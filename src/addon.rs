use dashmap::DashMap;
use ropey::Rope;
use sqf::analyzer::State;
use sqf::cpp::analyze_addon;
use sqf::span::{Span, Spanned};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

use crate::analyze::compute;
use crate::semantic_token::SemanticTokenLocation;

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
        let functions = functions
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    v.map(|x| {
                        let path = string_to_path(&x);
                        addon_path.parent().unwrap().join(path)
                    }),
                )
            })
            .collect();

        return Some((addon_path, functions));
    }
    None
}

fn string_to_path(path: &str) -> PathBuf {
    let mut new: PathBuf = PathBuf::new();
    for path in path.split('\\') {
        new.push(path)
    }
    new
}

#[derive(Debug, PartialEq, Clone)]
pub struct Error {
    pub inner: String,
    pub span: Span,
    pub url: Url,
}

pub type Documents = DashMap<String, Rope>;
pub type States = DashMap<String, Option<(State, Vec<SemanticTokenLocation>)>>;

pub fn process_addon(addon_path: PathBuf, functions: &Functions) -> Vec<Error> {
    let mut errors = vec![];
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

        let (_, new_errors) = compute(&content, path.inner.clone(), functions.keys().cloned());
        errors.extend(new_errors.into_iter().map(|x| Error {
            inner: x.inner,
            span: x.span,
            url: Url::from_file_path(&path.inner).unwrap(),
        }));
    }

    errors
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_add() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/addons/test/fn_basic.sqf");

        let (path, functions) = identify_addon(&Url::from_file_path(path).unwrap()).unwrap();
        let errors = process_addon(path, &functions);
        assert_eq!(errors, vec![]);
    }
}
