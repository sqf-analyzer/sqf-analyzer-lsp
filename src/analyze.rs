use std::path::PathBuf;

use sqf::analyzer::{analyze, MissionNamespace, State};
use sqf::error::Error;
use sqf::parser::parse;
use sqf::preprocessor::AstIterator;

use crate::semantic_token::{semantic_tokens, SemanticTokenLocation};

type Return = (State, Vec<SemanticTokenLocation>, Vec<Error>);

pub fn compute(text: &str, path: PathBuf, mission: MissionNamespace) -> Result<Return, Error> {
    let ast = sqf::preprocessor::parse(text)?;

    let semantic_tokens = semantic_tokens(&ast);
    let iter = AstIterator::new(ast, Default::default(), path);
    let (ast, mut errors) = parse(iter);
    let mut state = State::default();
    state.namespace.mission = mission;
    analyze(&ast, &mut state);
    errors.extend(state.errors.clone());
    Ok((state, semantic_tokens, errors))
}
