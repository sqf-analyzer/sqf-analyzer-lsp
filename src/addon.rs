use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use sqf::analyzer::{Configuration, Origin, Output, State};
use sqf::cpp::analyze_file;
use sqf::error::Error;
use sqf::preprocessor;
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
        let configuration = preprocessor::Configuration::with_path(addon_path.join("config.cpp"));
        let Ok((functions, _)) = analyze_file(configuration) else {
            continue
        };
        return Some((addon_path.join("config.cpp"), functions));
    }
    None
}

pub fn identify_mission(url: &Url) -> Option<(PathBuf, Functions)> {
    let mut addon_path = url.to_file_path().ok()?;
    while addon_path.pop() {
        let configuration =
            preprocessor::Configuration::with_path(addon_path.join("description.ext"));
        let Ok((functions, _)) = analyze_file(configuration) else {
            continue
        };
        return Some((addon_path.join("description.ext"), functions));
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
    configuration: Configuration,
    span: Span,
    functions: &Functions,
) -> R {
    let mut errors = vec![];
    let Ok(content) = std::fs::read_to_string(&configuration.file_path) else {
        errors.push(Error::new(
            format!("The function \"{}\" is defined but the file \"{}\" does not exist", function_name, configuration.file_path.display()),
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
    let (state, semantic_state, new_errors) = match compute(&content, configuration, mission) {
        Ok(a) => a,
        Err(e) => {
            errors.push(e);
            return (Some(content), errors, None);
        }
    };

    errors.extend(new_errors);

    (Some(content), errors, Some((state, semantic_state)))
}

type R2 = HashMap<Arc<Path>, (Arc<UncasedStr>, (State, Vec<SemanticTokenLocation>))>;

type R1 = (R2, HashMap<Arc<Path>, (String, Vec<Error>)>);

pub fn process(addon_path: PathBuf, functions: &Functions) -> R1 {
    let results = functions
        .par_iter()
        .filter_map(|(name, path)| {
            let span = path.span;

            let path = sqf::get_path(&path.inner, &addon_path, &Default::default()).ok()?;
            let configuration = Configuration {
                file_path: path.clone(),
                base_path: addon_path.to_owned(),
                ..Default::default()
            };

            Some((
                path,
                name.clone(),
                process_file(name.clone(), configuration, span, functions),
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
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            originals.insert(path, (content, errors));
        }
    }

    (states, originals)
}
