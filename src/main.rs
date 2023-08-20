use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use dashmap::DashMap;

use ropey::Rope;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use sqf::analyzer::{Origin, State};
use sqf::error::{Error, ErrorType};
use sqf::UncasedStr;
use sqf_analyzer_server::{addon, hover};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Notification;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use sqf_analyzer_server::semantic_token::SemanticTokenLocation;
use sqf_analyzer_server::{analyze::compute, definition, semantic_token::LEGEND_TYPE};

type States = DashMap<String, ((State, Vec<SemanticTokenLocation>), Option<Arc<UncasedStr>>)>;

#[derive(Debug)]
struct Backend {
    client: Client,
    states: States,
    documents: DashMap<String, Rope>,
    undefined_variables_are_error: AtomicBool,
    private_variables_in_mission_are_error: AtomicBool,
    is_loaded: AtomicBool,
    addon_path: RwLock<Option<Arc<Path>>>,
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
                hover_provider: Some(true.into()),
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

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        Ok(self.hover(params))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(self.get_definition(params))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        Ok(self.semantic(params).map(|semantic_token| {
            SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_token,
            })
        }))
    }

    async fn inlay_hint(
        &self,
        params: tower_lsp::lsp_types::InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        Ok(self.inlay(params))
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.client
            .log_message(MessageType::INFO, format!("{:?}", params.settings))
            .await;
        let variables = params
            .settings
            .as_object()
            .and_then(|x| x.get("sqf-analyzer"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("server"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("variables"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        self.undefined_variables_are_error
            .store(variables, Ordering::Relaxed);
        let variables = params
            .settings
            .as_object()
            .and_then(|x| x.get("sqf-analyzer"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("server"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("private_variables_in_mission_are_error"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        self.private_variables_in_mission_are_error
            .store(variables, Ordering::Relaxed);
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
            let rope = self.documents.get(uri.as_str())?;
            let offset = position_to_offset(params.text_document_position_params.position, &rope)?;

            let def = definition::get_definition(&state.0 .0, offset);

            def.and_then(|origin| match origin {
                Origin::File(span) => {
                    let range = span_to_range(span, &rope)?;
                    Some(GotoDefinitionResponse::Scalar(Location::new(
                        uri.clone(),
                        range,
                    )))
                }
                Origin::External(path, span) => {
                    let url = Url::from_file_path(path.as_ref()).ok()?;
                    let range = self
                        .documents
                        .get(url.as_str())
                        .and_then(|rope| span_to_range(span?, &rope))
                        .unwrap_or_default();
                    Some(GotoDefinitionResponse::Scalar(Location::new(url, range)))
                }
            })
        })
    }

    /// Loads the project for the first time, publishing any diagnostics it can find during the process
    async fn load_project(&self, uri: &Url, version: i32) {
        if self.is_loaded.load(Ordering::Relaxed) {
            return;
        };
        self.is_loaded.store(true, Ordering::Relaxed);
        self.client
            .log_message(MessageType::INFO, "loading mission or addon")
            .await;

        // collect functions and other scripts
        let (addon_path, functions) =
            if let Some((path, functions)) = addon::identify(uri, "config.cpp") {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "Found addon at \"{}\" with {} functions.",
                            path.display(),
                            functions.len()
                        ),
                    )
                    .await;
                (path, functions)
            } else if let Some((path, functions)) = addon::identify(uri, "description.ext") {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "Found mission at \"{}\" with {} functions.",
                            path.display(),
                            functions.len()
                        ),
                    )
                    .await;
                (path, functions)
            } else {
                self.client
                    .log_message(MessageType::INFO, "neither mission nor addon found")
                    .await;
                return;
            };

        let (states, originals) = addon::process(addon_path.clone(), &functions);

        {
            let mut w = self.addon_path.write().unwrap();
            *w = Some(addon_path.into());
        }

        for (path, (function_name, state_semantic)) in states {
            if let Ok(url) = Url::from_file_path(path) {
                self.states
                    .insert(url.to_string(), (state_semantic, function_name));
            }
        }

        let error_on_undefined = self.undefined_variables_are_error.load(Ordering::Relaxed);
        let private_variables_in_mission_are_error = self
            .private_variables_in_mission_are_error
            .load(Ordering::Relaxed);

        let diagnostics = originals
            .into_iter()
            // convert path to url. This is likely never filtered since originals only contain files that we could open
            .filter_map(|x| Url::from_file_path(x.0).ok().map(|url| (url, x.1)))
            // filter the current file because it may have not been saved and thus cannot be analyzed
            .filter(|(url, _)| url != uri)
            .flat_map(|(url, (content, errors))| {
                let rope = Rope::from_str(&content);
                errors
                    .into_iter()
                    .filter(|error| {
                        error_on_undefined || (error.type_ != ErrorType::UndefinedVariable)
                    })
                    .filter(|error| {
                        private_variables_in_mission_are_error
                            || (error.type_ != ErrorType::PrivateAssignedToMission)
                    })
                    .filter_map(|error| {
                        let origin = error
                            .origin
                            .clone()
                            .and_then(|x| Url::from_file_path(x).ok())
                            .unwrap_or_else(|| url.clone());
                        to_diagnostic(error, &rope).map(|x| (origin, x))
                    })
                    .collect::<Vec<_>>()
            })
            // group errors by files.
            // files may have errors from other files and thus need to be grouped by file
            .fold(
                std::collections::BTreeMap::<_, Vec<_>>::new(),
                |mut acc, (a, b)| {
                    acc.entry(a).or_default().push(b);
                    acc
                },
            );

        // todo: use futures join to push them concurrently
        for (url, diagnostics) in diagnostics {
            self.client
                .publish_diagnostics(url, diagnostics, Some(version))
                .await;
        }
    }

    async fn on_change(&self, params: TextDocumentItem) {
        self.load_project(&params.uri, params.version).await;

        let uri = &params.uri;
        let key = uri.to_string();
        self.documents
            .insert(key.clone(), ropey::Rope::from_str(&params.text));

        let mission = self
            .states
            .iter()
            .filter(|x| x.key().as_ref() != key)
            .flat_map(|x| x.0 .0.globals(x.1.clone()))
            .collect();

        let path = uri.to_file_path().expect("utf-8 path");
        let configuration = sqf::analyzer::Configuration {
            file_path: path.into(),
            base_path: self
                .addon_path
                .read()
                .unwrap()
                .as_ref()
                .map(|x| x.as_ref().to_owned())
                .unwrap_or_default(),
            ..Default::default()
        };

        let error_on_undefined = self.undefined_variables_are_error.load(Ordering::Relaxed);
        let private_variables_in_mission_are_error = self
            .private_variables_in_mission_are_error
            .load(Ordering::Relaxed);
        let (state_semantic, errors) = match compute(&params.text, configuration, mission) {
            Ok((state, semantic, errors)) => (Some((state, semantic)), errors),
            Err(e) => (None, vec![e]),
        };

        let url = params.uri.clone();
        let diagnostics = errors
            .into_iter()
            .filter(|error| error_on_undefined || (error.type_ != ErrorType::UndefinedVariable))
            .filter(|error| {
                private_variables_in_mission_are_error
                    || (error.type_ != ErrorType::PrivateAssignedToMission)
            })
            .filter_map(|error| {
                let origin = error
                    .origin
                    .clone()
                    .and_then(|x| Url::from_file_path(x).ok())
                    .unwrap_or_else(|| url.clone());
                let rope = self.documents.get(origin.as_str())?;
                to_diagnostic(error, &rope).map(|x| (origin, x))
            })
            .fold(
                std::collections::BTreeMap::<_, Vec<_>>::new(),
                |mut acc, (a, b)| {
                    acc.entry(a).or_default().push(b);
                    acc
                },
            );

        if diagnostics.is_empty() {
            self.client
                .publish_diagnostics(params.uri.clone(), vec![], Some(params.version))
                .await;
        }
        for (url, diagnostics) in diagnostics {
            self.client
                .publish_diagnostics(url, diagnostics, Some(params.version))
                .await;
        }

        if let Some(state_semantic) = state_semantic {
            let key = params.uri.to_string();
            if let Some(mut e) = self.states.get_mut(&key) {
                e.value_mut().0 = state_semantic;
            } else {
                self.states.insert(key, (state_semantic, None));
            };
        }
    }

    fn hover(&self, params: HoverParams) -> Option<Hover> {
        let uri = params.text_document_position_params.text_document.uri;

        let rope = self.documents.get(uri.as_str())?;

        let state = &self.states.get(uri.as_str())?.0 .0;

        let offset = position_to_offset(params.text_document_position_params.position, &rope)?;

        hover::hover(state, offset).map(|explanation| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: explanation.to_string(),
            }),
            range: None,
        })
    }

    fn inlay(&self, params: tower_lsp::lsp_types::InlayHintParams) -> Option<Vec<InlayHint>> {
        let uri = &params.text_document.uri;

        let document = self.documents.get(uri.as_str())?;

        let state = &self.states.get(uri.as_str())?.0 .0;

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

    fn semantic(&self, params: SemanticTokensParams) -> Option<Vec<SemanticToken>> {
        let uri = params.text_document.uri.as_str();
        let im_complete_tokens = &self.states.get(uri)?.0 .1;
        let rope = self.documents.get(uri)?;
        let mut previous_line = 0;
        let mut previous_start = 0;
        let semantic_tokens = im_complete_tokens
            .iter()
            .filter_map(|token| {
                let line = rope.try_byte_to_line(token.start).ok()? as u32;
                let first = rope.try_line_to_char(line as usize).ok()? as u32;
                let start = rope.try_byte_to_char(token.start).ok()? as u32 - first;
                let delta_line = line - previous_line;
                let delta_start = if delta_line == 0 {
                    start - previous_start
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
                previous_line = line;
                previous_start = start;
                ret
            })
            .collect::<Vec<_>>();
        Some(semantic_tokens)
    }
}

fn to_diagnostic(item: Error, rope: &Rope) -> Option<Diagnostic> {
    let (message, span) = (item.type_.to_string(), item.span);
    let start_position = offset_to_position(span.0, rope)?;
    let end_position = offset_to_position(span.1, rope)?;
    Some(Diagnostic::new(
        Range::new(start_position, end_position),
        None,
        None,
        Some("sqf-analyzer".into()),
        message,
        None,
        None,
    ))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        undefined_variables_are_error: false.into(),
        private_variables_in_mission_are_error: false.into(),
        is_loaded: false.into(),
        states: Default::default(),
        documents: Default::default(),
        addon_path: Default::default(),
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
