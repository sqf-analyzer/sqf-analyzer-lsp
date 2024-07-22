use sqf::analyzer::{Namespace, BINARY, NULLARY, UNARY};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

pub(super) fn completion(namespace: &Namespace) -> Vec<CompletionItem> {
    namespace
        .stack
        .iter()
        .map(|stack| stack.variables.keys())
        .flatten()
        .map(|var| CompletionItem {
            label: var.to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            ..Default::default()
        })
        .chain(NULLARY.iter().map(|(var, (type_, detail))| CompletionItem {
            label: var.to_string(),
            kind: Some(CompletionItemKind::CONSTANT),
            detail: Some(detail.to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("* `{type_:?}`: {}", detail.to_string()),
            })),
            ..Default::default()
        }))
        .chain(UNARY.iter().map(|(var, variants)| {
            CompletionItem {
                label: var.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: variants
                    .iter()
                    .next()
                    .and_then(|(_, value)| value.get(0).map(|x| x.1.to_string())),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: variants
                        .iter()
                        .map(|(type_, value)| {
                            value.iter().map(move |(t, explanation)| {
                                format!("* `{} {:?} -> {:?}`: {}", var, type_, t, explanation)
                            })
                        })
                        .flatten()
                        .collect::<Vec<String>>()
                        .join("\n"),
                })),
                ..Default::default()
            }
        }))
        .chain(BINARY.iter().map(|(var, variants)| {
            CompletionItem {
                label: var.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: variants
                    .iter()
                    .next()
                    .and_then(|(_, value)| value.get(0).map(|x| x.1.to_string())),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: variants
                        .iter()
                        .map(|(type_, value)| {
                            value.iter().map(|(t, explanation)| {
                                format!(
                                    "* `{:?} {} {:?} -> {:?}`: {}",
                                    type_.0, var, type_.1, t, explanation,
                                )
                            })
                        })
                        .flatten()
                        .collect::<Vec<String>>()
                        .join("\n"),
                })),
                ..Default::default()
            }
        }))
        .collect()
}
