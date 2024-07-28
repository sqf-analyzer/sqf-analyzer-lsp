use sqf::{
    analyzer::{Namespace, Output, Parameter, BINARY, NULLARY, UNARY},
    types::Type,
};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

fn params_to_string(params: &Vec<Parameter>) -> String {
    format!(
        "[{}]",
        params
            .iter()
            .map(|param| format!("{}: {:?}", param.name, param.type_))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn output_to_string(output: &Option<Output>) -> Option<String> {
    output.as_ref().map(|output| match output {
        Output::Type(type_) => format!("{type_:?}"),
        Output::Code(params, output) => params
            .as_ref()
            .map(params_to_string)
            .map(|param| output.map(|o| format!("{param} -> {o:?}")).unwrap_or(param))
            .unwrap_or_else(|| format!("{:?}", Type::Code)),
    })
}

pub(super) fn completion(namespace: &Namespace) -> Vec<CompletionItem> {
    namespace
        .stack
        .iter()
        .map(|stack| stack.variables.iter())
        .flatten()
        .map(|(var, (_, output))| CompletionItem {
            label: var.to_string(),
            kind: Some(
                matches!(output.as_ref(), Some(Output::Code(_, _)))
                    .then(|| CompletionItemKind::VARIABLE)
                    .unwrap_or_else(|| CompletionItemKind::FUNCTION),
            ),
            detail: output_to_string(output),
            ..Default::default()
        })
        .chain(namespace.mission.iter().map(|(var, (_, output))| {
            CompletionItem {
                label: var.to_string(),
                kind: Some(
                    matches!(output.as_ref(), Some(Output::Code(_, _)))
                        .then(|| CompletionItemKind::VARIABLE)
                        .unwrap_or_else(|| CompletionItemKind::FUNCTION),
                ),
                detail: output_to_string(output),
                ..Default::default()
            }
        }))
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
