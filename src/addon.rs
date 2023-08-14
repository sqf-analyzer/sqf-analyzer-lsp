use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use rayon::prelude::*;
use ropey::Rope;
use sqf;
use sqf::analyzer::{Origin, Output, Parameter, State};
use sqf::cpp::analyze_file;
use sqf::error::Error;
use sqf::span::{Span, Spanned};
use sqf::types::Type;
use tower_lsp::lsp_types::Url;

use crate::analyze::compute;
use crate::semantic_token::SemanticTokenLocation;

pub type Signature = (Spanned<PathBuf>, Option<Vec<Parameter>>, Option<Type>);
type Signatures = HashMap<Arc<str>, Signature>;
type Functions = HashMap<Arc<str>, Spanned<String>>;

pub fn identify_addon(url: &Url) -> Option<(PathBuf, Functions)> {
    let mut addon_path = url.to_file_path().ok()?;
    while addon_path.pop() {
        let Ok((functions, errors)) = analyze_file(addon_path.join("config.cpp").clone()) else {
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
        let Ok((functions, errors)) = analyze_file(addon_path.join("description.ext").clone()) else {
            continue
        };
        if !errors.is_empty() {
            return None;
        }

        return Some((addon_path, functions));
    }
    None
}

pub type Documents = DashMap<String, Rope>;
pub type States = DashMap<String, Option<(State, Vec<SemanticTokenLocation>)>>;

type R = (
    Option<String>,
    Vec<Error>,
    Spanned<PathBuf>,
    Option<Vec<Parameter>>,
    Option<Type>,
);

fn process_file(
    function_name: Arc<str>,
    path: PathBuf,
    functions_path: PathBuf,
    span: Span,
    functions: &Functions,
) -> R {
    let mut errors = vec![];
    let Ok(content) = std::fs::read_to_string(&path) else {
        //let url = Url::from_file_path(addon_path.join("config.cpp")).expect("todo: non-utf8 paths");
        errors.push(Error {
            inner: format!("The function \"{}\" is defined but the file \"{}\" does not exist", function_name, path.display()),
            span,
        });
        return (None, errors, Spanned {
            inner: functions_path,
            span,
        }, None, None)
    };

    let origins = functions.iter().map(|(k, _)| {
        (
            k.clone(),
            (
                Origin::External(k.clone(), None),
                Some(Output::Type(Type::Code)),
            ),
        )
    });
    let (state, _, new_errors) = match compute(&content, path.clone(), origins) {
        Ok(a) => a,
        Err(e) => {
            errors.push(e);
            return (
                Some(content),
                errors,
                Spanned { inner: path, span },
                None,
                None,
            );
        }
    };

    errors.extend(new_errors);

    (
        Some(content),
        errors,
        Spanned { inner: path, span },
        state.signature().cloned(),
        state.return_type(),
    )
}

type R1 = (Signatures, HashMap<PathBuf, (String, Vec<Error>)>);

pub fn process_addon(addon_path: PathBuf, functions: &Functions) -> R1 {
    process(addon_path, functions, "config.cpp")
}

pub fn process_mission(addon_path: PathBuf, functions: &Functions) -> R1 {
    process(addon_path, functions, "description.ext")
}

fn process(addon_path: PathBuf, functions: &Functions, file_name: &'static str) -> R1 {
    let functions_path = addon_path.join(file_name);
    let results = functions
        .par_iter()
        .filter_map(|(name, path)| {
            let span = path.span;
            let path = sqf::get_path(&path.inner, functions_path.clone()).ok()?;

            Some((
                name.clone(),
                process_file(name.clone(), path, functions_path.clone(), span, functions),
            ))
        })
        .collect::<Vec<_>>();

    let mut signatures = Signatures::default();
    let mut originals = HashMap::default(); // todo: remove this so we do not store all files
    for (name, (content, errors, path, signature, return_type)) in results {
        signatures.insert(name.clone(), (path.clone(), signature, return_type));
        if let Some(content) = content {
            originals.insert(path.inner, (content, errors));
        } else if let Ok(content) = std::fs::read_to_string(&functions_path) {
            originals.insert(functions_path.clone(), (content, errors));
        }
    }

    (signatures, originals)
}
