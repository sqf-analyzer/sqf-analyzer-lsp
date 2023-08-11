use dashmap::DashMap;
use ropey::Rope;
use sqf::analyzer::{Origin, Output, Parameter, State};
use sqf::cpp::analyze_addon;
use sqf::span::{Span, Spanned};
use sqf::types::Type;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

use crate::analyze::compute;
use crate::semantic_token::SemanticTokenLocation;

type Signatures = HashMap<Arc<str>, (Spanned<PathBuf>, Vec<Parameter>)>;
type Functions = HashMap<Arc<str>, Spanned<PathBuf>>;

pub fn identify_addon(url: &Url) -> Option<(PathBuf, Functions)> {
    let mut addon_path = url.to_file_path().ok()?;
    while addon_path.pop() {
        let Ok((functions, errors)) = analyze_addon(addon_path.clone()) else {
            continue
        };
        if !errors.is_empty() {
            return None;
        }

        return Some((addon_path, functions));
    }
    None
}

#[derive(Debug, PartialEq, Clone)]
pub struct Error {
    pub inner: String,
    pub span: Span,
    pub url: Url,
}

pub type Documents = DashMap<String, Rope>;
pub type States = DashMap<String, Option<(State, Vec<SemanticTokenLocation>)>>;

pub fn process_addon(addon_path: PathBuf, functions: &Functions) -> (Signatures, Vec<Error>) {
    let mut errors = vec![];
    let mut signatures = Signatures::default();
    for (name, path) in functions {
        let Ok(content) = std::fs::read_to_string(&path.inner) else {
            let url = Url::from_file_path(addon_path.join("config.cpp")).expect("todo: non-utf8 paths");
            errors.push(Error {
                inner: format!("The function \"{}\" is defined but the file \"{}\" does not exist", name, path.inner.display()),
                span: path.span,
                url,
            });
            continue
        };

        let origins = functions.iter().map(|(k, _)| {
            (
                k.clone(),
                (Origin::External(k.clone()), Some(Output::Type(Type::Code))),
            )
        });
        let (state, _, new_errors) = match compute(&content, path.inner.clone(), origins) {
            Ok(a) => a,
            Err(e) => {
                errors.push(Error {
                    inner: e.inner,
                    span: e.span,
                    url: Url::from_file_path(&path.inner).unwrap(),
                });
                continue;
            }
        };

        errors.extend(new_errors.into_iter().map(|x| Error {
            inner: x.inner,
            span: x.span,
            url: Url::from_file_path(&path.inner).unwrap(),
        }));

        if let Some(signature) = state.signature() {
            signatures.insert(name.clone(), (path.clone(), signature.to_vec()));
        }
    }

    (signatures, errors)
}
