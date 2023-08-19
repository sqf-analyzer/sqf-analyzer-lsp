use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use sqf::analyzer::{Configuration, Origin, Output, State};
use sqf::cpp::analyze_file;
use sqf::error::Error;
use sqf::span::Spanned;
use sqf::types::Type;
use sqf::{self, UncasedStr, MISSION_INIT_SCRIPTS};
use sqf::{get_path, preprocessor};
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

fn process_file(content: String, configuration: Configuration, functions: &Functions) -> R {
    let mut errors = vec![];

    let mission = functions
        .iter()
        .filter_map(|(k, path)| {
            let Ok(path) = get_path(&path.inner, &configuration.base_path, &configuration.addons) else {
                return None
            };
            Some((
                k.clone(),
                (
                    Origin::External(path, None),
                    Some(Output::Type(Type::Code)),
                ),
            ))
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

type R2 = HashMap<Arc<Path>, (Option<Arc<UncasedStr>>, (State, Vec<SemanticTokenLocation>))>;

type R1 = (R2, HashMap<Arc<Path>, (String, Vec<Error>)>);

enum Either {
    Original(Spanned<String>),
    Path(Arc<Path>),
}

pub fn process(addon_path: PathBuf, functions: &Functions) -> R1 {
    let f = functions.par_iter().map(|(function_name, sqf_path)| {
        let path = get_path(&sqf_path.inner, &addon_path, &Default::default()).ok();
        (
            Some(Spanned::new(function_name.clone(), sqf_path.span)),
            path.map(Either::Path)
                .unwrap_or(Either::Original(sqf_path.clone())),
        )
    });
    // iterator over default files analyze
    let defaults = MISSION_INIT_SCRIPTS.into_par_iter().map(|file| {
        let mut directory = addon_path.to_owned();
        directory.pop();
        let path: Arc<Path> = directory.join(file).into();
        (None::<Spanned<Arc<UncasedStr>>>, Either::Path(path))
    });

    // iterator over all relevant files to analyze
    let files = f.chain(defaults);

    let results = files
        .filter_map(|(function_name, path)| {
            let (path, content) = match path {
                Either::Original(original) => {
                    let processed = (None, vec![Error::new(
                        format!("The function \"{}\" is declared but could not derive a path for \"{}\"", function_name.as_ref().unwrap().inner, original.inner),
                        original.span,
                    )], None);
                    return Some((addon_path.clone().into(), function_name.map(|x| x.inner), processed));
                }
                Either::Path(path) => {
                    let Ok(content) = std::fs::read_to_string(path.as_ref()) else {
                        if let Some(ma) = &function_name {
                            let processed = (None, vec![Error::new(
                                format!("The function \"{}\" is declared but could not open file \"{}\"", ma.inner, path.display()),
                                ma.span,
                            )], None);

                            return Some((path, function_name.map(|x| x.inner), processed));
                        } else {
                            // default files are optional, skip if not found
                            return None
                        };
                    };
                    (path, content)
                },
            };

            let configuration = Configuration {
                file_path: path.clone(),
                base_path: addon_path.to_owned(),
                ..Default::default()
            };

            Some((
                path,
                function_name.map(|x| x.inner),
                process_file(content, configuration, functions),
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
