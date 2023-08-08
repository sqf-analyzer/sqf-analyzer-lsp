use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;

use ropey::Rope;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use sqf::analyzer::{Origin, Output, Parameter};
use sqf::span::Spanned;
use sqf_analyzer_server::addon;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Notification;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use sqf_analyzer_server::{analyze::compute, definition, semantic_token::LEGEND_TYPE};

#[derive(Debug)]
struct Backend {
    client: Client,
    states: addon::States,
    functions: DashMap<Arc<str>, (Spanned<PathBuf>, Vec<Parameter>)>, // addon functions defined in config.cpp (name, path)
    documents: DashMap<String, Rope>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            offset_encoding: None,
            capabilities: ServerCapabilities {
                inlay_hint_provider: Some(OneOf::Left(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: None,
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["dummy.do_something".to_string()],
                    work_done_progress_options: Default::default(),
                }),

                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: {
                                TextDocumentRegistrationOptions {
                                    document_selector: Some(vec![DocumentFilter {
                                        language: Some("sqf".to_string()),
                                        scheme: Some("file".to_string()),
                                        pattern: None,
                                    }]),
                                }
                            },
                            semantic_tokens_options: SemanticTokensOptions {
                                work_done_progress_options: WorkDoneProgressOptions::default(),
                                legend: SemanticTokensLegend {
                                    token_types: LEGEND_TYPE.into(),
                                    token_modifiers: vec![],
                                },
                                range: Some(false),
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            static_registration_options: StaticRegistrationOptions::default(),
                        },
                    ),
                ),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(false)),
                rename_provider: Some(OneOf::Left(false)),
                ..ServerCapabilities::default()
            },
        })
    }
    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file \"{}\" opened", params.text_document.uri),
            )
            .await;
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
            version: params.text_document.version,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file \"{}\" changed", params.text_document.uri),
            )
            .await;
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
        })
        .await
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file saved!")
            .await;
    }
    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.client
            .log_message(MessageType::INFO, "goto_definition")
            .await;
        Ok(self.get_definition(params))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.client
            .log_message(MessageType::INFO, "semantic_tokens_full")
            .await;
        let uri = params.text_document.uri.as_str();
        let semantic_tokens = || -> Option<Vec<SemanticToken>> {
            let may = self.states.get(uri)?;
            let im_complete_tokens = &may.as_ref()?.1;
            let rope = self.documents.get(uri)?;
            let mut pre_line = 0;
            let mut pre_start = 0;
            let semantic_tokens = im_complete_tokens
                .iter()
                .filter_map(|token| {
                    let line = rope.try_byte_to_line(token.start).ok()? as u32;
                    let first = rope.try_line_to_char(line as usize).ok()? as u32;
                    let start = rope.try_byte_to_char(token.start).ok()? as u32 - first;
                    let delta_line = line - pre_line;
                    let delta_start = if delta_line == 0 {
                        start - pre_start
                    } else {
                        start
                    };
                    let ret = Some(SemanticToken {
                        delta_line,
                        delta_start,
                        length: token.length as u32,
                        token_type: token.token_type as u32,
                        token_modifiers_bitset: 0,
                    });
                    pre_line = line;
                    pre_start = start;
                    ret
                })
                .collect::<Vec<_>>();
            Some(semantic_tokens)
        }();
        if let Some(semantic_token) = semantic_tokens {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_token,
            })));
        }
        Ok(None)
    }

    async fn inlay_hint(
        &self,
        params: tower_lsp::lsp_types::InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        self.client
            .log_message(MessageType::INFO, "inlay hint")
            .await;
        Ok(self.inlay(params))
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.client
            .log_message(MessageType::INFO, "configuration changed!")
            .await;
    }

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(MessageType::INFO, "workspace folders changed!")
            .await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.client
            .log_message(MessageType::INFO, "watched files have changed!")
            .await;
    }

    async fn execute_command(&self, _: ExecuteCommandParams) -> Result<Option<Value>> {
        self.client
            .log_message(MessageType::INFO, "command executed!")
            .await;

        match self.client.apply_edit(WorkspaceEdit::default()).await {
            Ok(res) if res.applied => self.client.log_message(MessageType::INFO, "applied").await,
            Ok(_) => self.client.log_message(MessageType::INFO, "rejected").await,
            Err(err) => self.client.log_message(MessageType::ERROR, err).await,
        }

        Ok(None)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct InlayHintParams {
    path: String,
}

enum CustomNotification {}
impl Notification for CustomNotification {
    type Params = InlayHintParams;
    const METHOD: &'static str = "custom/notification";
}
struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

fn span_to_range((start, end): (usize, usize), rope: &Rope) -> Option<Range> {
    let start_position = offset_to_position(start, rope)?;
    let end_position = offset_to_position(end, rope)?;

    Some(Range::new(start_position, end_position))
}

impl Backend {
    fn get_definition(&self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let uri = params.text_document_position_params.text_document.uri;
        self.states.get(uri.as_str()).and_then(|state| {
            let rope = self.documents.get(uri.as_str()).unwrap();
            let offset = position_to_offset(params.text_document_position_params.position, &rope)?;

            let def = definition::get_definition(&state.as_ref()?.0, offset);

            def.and_then(|origin| match origin {
                Origin::File(span) => {
                    let range = span_to_range(span, &rope)?;
                    Some(GotoDefinitionResponse::Scalar(Location::new(
                        uri.clone(),
                        range,
                    )))
                }
                Origin::External(origin) => self.functions.get(&origin).map(|path| {
                    let uri = Url::from_file_path(&path.0.inner).unwrap();
                    GotoDefinitionResponse::Scalar(Location::new(uri, Range::default()))
                }),
            })
        })
    }

    async fn on_change(&self, params: TextDocumentItem) {
        let uri = &params.uri;
        let rope = ropey::Rope::from_str(&params.text);
        self.documents.insert(params.uri.to_string(), rope.clone());

        let origins = self.functions.iter().map(|x| {
            (
                x.key().clone(),
                (
                    Origin::External(x.key().clone()),
                    Some(Output::Code(x.value().1.clone())),
                ),
            )
        });
        let path = uri.to_file_path().expect("utf-8 path");
        let s = compute(&params.text, path.clone(), origins);

        let (state_semantic, errors) = match s {
            Ok((state, semantic, errors)) => (Some((state, semantic)), errors),
            Err(e) => (None, vec![e]),
        };

        let diagnostics = errors
            .into_iter()
            .filter_map(|item| {
                let (message, span) = (item.inner, item.span);

                || -> Option<Diagnostic> {
                    let start_position = offset_to_position(span.0, &rope)?;
                    let end_position = offset_to_position(span.1, &rope)?;
                    Some(Diagnostic::new_simple(
                        Range::new(start_position, end_position),
                        message,
                    ))
                }()
            })
            .collect::<Vec<_>>();

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;

        self.states.insert(params.uri.to_string(), state_semantic);

        let Some((path, functions)) = addon::identify_addon(uri) else {
            return
        };
        let (signatures, errors) = addon::process_addon(path, &functions);

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Found addon with {} functions. Example: \"{}\"",
                    functions.len(),
                    functions
                        .keys()
                        .next()
                        .map(|x| x.as_ref())
                        .unwrap_or_default()
                ),
            )
            .await;

        self.functions.clear();
        for (a, b) in signatures {
            self.functions.insert(a, b);
        }

        let diagnostics = errors
            .into_iter()
            .filter_map(|item| {
                let (message, span) = (item.inner, item.span);

                let start_position = offset_to_position(span.0, &rope)?;
                let end_position = offset_to_position(span.1, &rope)?;
                Some((
                    item.url,
                    Diagnostic::new_simple(Range::new(start_position, end_position), message),
                ))
            })
            // filter the current file because it may have not been saved and thus cannot be analyzed
            .filter(|x| x.0 != params.uri)
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut acc, (a, b)| {
                acc.entry(a).or_default().push(b);
                acc
            });

        // todo: use futures join to push them concurrently
        for (item, diagnostics) in diagnostics {
            self.client
                .publish_diagnostics(item, diagnostics, Some(params.version))
                .await;
        }
    }

    fn inlay(&self, params: tower_lsp::lsp_types::InlayHintParams) -> Option<Vec<InlayHint>> {
        let uri = &params.text_document.uri;

        let document = match self.documents.get(uri.as_str()) {
            Some(rope) => rope,
            None => return None,
        };

        let state = self.states.get(uri.as_str())?;
        let state = &state.as_ref()?.0;

        let items = state
            .types
            .iter()
            .filter_map(|(k, v)| v.map(|v| (k, v)))
            .filter_map(|(span, type_)| {
                let end_position = offset_to_position(span.1, &document)?;
                let inlay_hint = InlayHint {
                    text_edits: None,
                    tooltip: None,
                    kind: Some(InlayHintKind::TYPE),
                    padding_left: None,
                    padding_right: None,
                    data: None,
                    position: end_position,
                    label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                        value: format!(": {type_:?}"),
                        tooltip: None,
                        location: Some(Location {
                            uri: params.text_document.uri.clone(),
                            range: Range {
                                start: Position::new(0, 4),
                                end: Position::new(0, 5),
                            },
                        }),
                        command: None,
                    }]),
                };
                Some(inlay_hint)
            });

        let params = state.parameters.iter().filter_map(|(span, name)| {
            let position = offset_to_position(span.0, &document)?;
            let inlay_hint = InlayHint {
                text_edits: None,
                tooltip: None,
                kind: Some(InlayHintKind::PARAMETER),
                padding_left: None,
                padding_right: None,
                data: None,
                position,
                label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                    value: format!("{name}: "),
                    tooltip: None,
                    location: Some(Location {
                        uri: params.text_document.uri.clone(),
                        range: Range {
                            start: Position::new(0, 4),
                            end: Position::new(0, 5),
                        },
                    }),
                    command: None,
                }]),
            };
            Some(inlay_hint)
        });

        Some(items.chain(params).collect())
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        functions: DashMap::new(),
        states: DashMap::new(),
        documents: DashMap::new(),
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}

fn offset_to_position(offset: usize, rope: &Rope) -> Option<Position> {
    let line = rope.try_char_to_line(offset).ok()?;
    let first_char_of_line = rope.try_line_to_char(line).ok()?;
    let column = offset - first_char_of_line;
    Some(Position::new(line as u32, column as u32))
}

fn position_to_offset(position: Position, rope: &Rope) -> Option<usize> {
    let char = rope.try_line_to_char(position.line as usize).ok()?;
    Some(char + position.character as usize)
}
