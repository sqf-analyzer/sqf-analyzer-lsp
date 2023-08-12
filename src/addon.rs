use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use rayon::prelude::*;
use ropey::Rope;
use sqf;
use sqf::analyzer::{Origin, Output, Parameter, State};
use sqf::cpp::{analyze_addon, analyze_mission};
use sqf::span::{Span, Spanned};
use sqf::types::Type;
use tower_lsp::lsp_types::Url;

use crate::analyze::compute;
use crate::semantic_token::SemanticTokenLocation;

pub type Signature = (Spanned<PathBuf>, Option<Vec<Parameter>>, Option<Type>);
type Signatures = HashMap<Arc<str>, Signature>;
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

pub fn identify_mission(url: &Url) -> Option<(PathBuf, Functions)> {
    let mut addon_path = url.to_file_path().ok()?;
    while addon_path.pop() {
        let Ok((functions, errors)) = analyze_mission(addon_path.clone()) else {
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

type R = (
    Vec<Error>,
    Spanned<PathBuf>,
    Option<Vec<Parameter>>,
    Option<Type>,
);

fn process_file(
    name: Arc<str>,
    path: PathBuf,
    span: Span,
    addon_path: PathBuf,
    functions: &Functions,
) -> Option<R> {
    let mut errors = vec![];
    let Ok(content) = std::fs::read_to_string(&path) else {
        let url = Url::from_file_path(addon_path.join("config.cpp")).expect("todo: non-utf8 paths");
        errors.push(Error {
            inner: format!("The function \"{}\" is defined but the file \"{}\" does not exist", name, path.display()),
            span,
            url,
        });
        return Some((errors, Spanned {
            inner: path.clone(),
            span,
        }, None, None))
    };

    let origins = functions.iter().map(|(k, _)| {
        (
            k.clone(),
            (Origin::External(k.clone()), Some(Output::Type(Type::Code))),
        )
    });
    let (state, _, new_errors) = match compute(&content, path.clone(), origins) {
        Ok(a) => a,
        Err(e) => {
            errors.push(Error {
                inner: e.inner,
                span: e.span,
                url: Url::from_file_path(&path).unwrap(),
            });
            return Some((
                errors,
                Spanned {
                    inner: path.clone(),
                    span,
                },
                None,
                None,
            ));
        }
    };

    errors.extend(new_errors.into_iter().map(|x| Error {
        inner: x.inner,
        span: x.span,
        url: Url::from_file_path(&path).unwrap(),
    }));

    Some((
        errors,
        Spanned {
            inner: path.clone(),
            span,
        },
        state.signature().cloned(),
        state.return_type(),
    ))
}

pub fn process_addon(addon_path: PathBuf, functions: &Functions) -> (Signatures, Vec<Error>) {
    let results = functions
        .par_iter()
        .filter_map(|(name, path)| {
            let span = path.span;
            let path = sqf::find_addon_path(&path.inner)?;

            process_file(name.clone(), path, span, addon_path.clone(), functions)
                .map(|x| (name.clone(), x))
        })
        .collect::<Vec<_>>();

    let mut errors = vec![];
    let mut signatures = Signatures::default();
    for (name, (e, path, signature, return_type)) in results {
        errors.extend(e);
        signatures.insert(name.clone(), (path, signature, return_type));
    }

    (signatures, errors)
}

pub fn process_mission(addon_path: PathBuf, functions: &Functions) -> (Signatures, Vec<Error>) {
    let results = functions
        .par_iter()
        .filter_map(|(name, path)| {
            let span = path.span;
            let path = sqf::find_mission_path(&path.inner)?;

            process_file(name.clone(), path, span, addon_path.clone(), functions)
                .map(|x| (name.clone(), x))
        })
        .collect::<Vec<_>>();

    let mut errors = vec![];
    let mut signatures = Signatures::default();
    for (name, (e, path, signature, return_type)) in results {
        errors.extend(e);
        signatures.insert(name.clone(), (path, signature, return_type));
    }

    (signatures, errors)
}
