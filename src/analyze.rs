use sqf::analyzer::{analyze, MissionNamespace, State};
use sqf::error::Error;
use sqf::parser::parse;
use sqf::preprocessor::AstIterator;

use crate::semantic_token::{semantic_tokens, SemanticTokenLocation};

type Return = (State, Vec<SemanticTokenLocation>, Vec<Error>);

pub fn compute(
    text: &str,
    configuration: sqf::analyzer::Configuration,
    mission: MissionNamespace,
) -> Result<Return, Error> {
    let ast = sqf::preprocessor::parse(text)?;
    let semantic_tokens = semantic_tokens(&ast, &mission);

    let conf = sqf::preprocessor::Configuration {
        path: configuration.file_path.clone(),
        addons: configuration.addons.clone(),
        ..Default::default()
    };

    let iter = AstIterator::new(ast, conf);
    let (ast, mut errors) = parse(iter);
    let mut state = State {
        configuration,
        ..Default::default()
    };
    state.namespace.mission = mission;
    analyze(&ast, &mut state);
    errors.extend(state.errors.clone());
    Ok((state, semantic_tokens, errors))
}
