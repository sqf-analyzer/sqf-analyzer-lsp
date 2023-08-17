use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rayon::prelude::*;
use sqf::analyzer::{Origin, Output, State};
use sqf::cpp::analyze_file;
use sqf::error::Error;
use sqf::preprocessor::Configuration;
use sqf::span::{Span, Spanned};
use sqf::types::Type;
use sqf::{self, UncasedStr};
use tower_lsp::lsp_types::Url;

use crate::analyze::compute;
use crate::semantic_token::SemanticTokenLocation;

type Functions = HashMap<Arc<UncasedStr>, Spanned<String>>;

pub fn identify_addon(url: &Url) -> Option<(PathBuf, Functions)> {
    let mut addon_path = url.to_file_path().ok()?;
    while addon_path.pop() {
        let configuration = Configuration::with_path(addon_path.join("config.cpp"));
        let Ok((functions, errors)) = analyze_file(configuration) else {
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
        let configuration = Configuration::with_path(addon_path.join("description.ext"));
        let Ok((functions, errors)) = analyze_file(configuration) else {
            continue
        };
        if !errors.is_empty() {
            return None;
        }

        return Some((addon_path, functions));
    }
    None
}

type R = (
    Option<String>,
    Vec<Error>,
    Option<(State, Vec<SemanticTokenLocation>)>,
);

fn process_file(
    function_name: Arc<UncasedStr>,
    path: PathBuf,
    span: Span,
    functions: &Functions,
) -> R {
    let mut errors = vec![];
    let Ok(content) = std::fs::read_to_string(&path) else {
        errors.push(Error::new(
            format!("The function \"{}\" is defined but the file \"{}\" does not exist", function_name, path.display()),
            span,
        ));
        return (None, errors, None)
    };

    let mission = functions
        .iter()
        .map(|(k, _)| {
            (
                k.clone(),
                (
                    Origin::External(k.clone(), None),
                    Some(Output::Type(Type::Code)),
                ),
            )
        })
        .collect();
    let (state, semantic_state, new_errors) = match compute(&content, path, mission, false) {
        Ok(a) => a,
        Err(e) => {
            errors.push(e);
            return (Some(content), errors, None);
        }
    };

    errors.extend(new_errors);

    (Some(content), errors, Some((state, semantic_state)))
}

type R2 = HashMap<PathBuf, (Arc<UncasedStr>, (State, Vec<SemanticTokenLocation>))>;

type R1 = (R2, HashMap<PathBuf, (String, Vec<Error>)>);

pub fn process_addon(addon_path: PathBuf, functions: &Functions) -> R1 {
    process(addon_path, functions, "config.cpp")
}

pub fn process_mission(addon_path: PathBuf, functions: &Functions) -> R1 {
    process(addon_path, functions, "description.ext")
}

fn process(addon_path: PathBuf, functions: &Functions, file_name: &'static str) -> R1 {
    let functions_path = addon_path.join(file_name);
    let configuration = Configuration::with_path(functions_path.clone());
    let results = functions
        .par_iter()
        .filter_map(|(name, path)| {
            let span = path.span;

            let path = sqf::get_path(&path.inner, &configuration).ok()?;

            Some((
                path.clone(),
                name.clone(),
                process_file(name.clone(), path, span, functions),
            ))
        })
        .collect::<Vec<_>>();

    let mut states: R2 = Default::default();
    let mut originals = HashMap::default(); // todo: remove this so we do not store all files
    for (path, name, (content, errors, state)) in results {
        if let Some(state) = state {
            states.insert(path.clone(), (name.clone(), state));
        }
        if let Some(content) = content {
            originals.insert(path, (content, errors));
        } else if let Ok(content) = std::fs::read_to_string(&functions_path) {
            originals.insert(functions_path.clone(), (content, errors));
        }
    }

    (states, originals)
}
