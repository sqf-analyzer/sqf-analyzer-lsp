use std::path::PathBuf;
use std::sync::Arc;

use sqf::analyzer::{analyze, Origin, State};
use sqf::error::Error;
use sqf::parser::parse;
use sqf::preprocessor::AstIterator;
use sqf::types::Type;

use crate::semantic_token::{semantic_tokens, SemanticTokenLocation};

type Return = (Option<(State, Vec<SemanticTokenLocation>)>, Vec<Error>);

pub fn compute(text: &str, path: PathBuf, origins: impl Iterator<Item = Arc<str>>) -> Return {
    let ast = sqf::preprocessor::parse(text);

    match ast {
        Ok(t) => {
            let semantic_tokens = semantic_tokens(&t);
            let iter = AstIterator::new(t, Default::default(), path);
            let (ast, mut errors) = parse(iter);
            let mut state = State::default();
            state
                .namespace
                .mission
                .extend(origins.map(|v| (v.clone(), (Origin::External(v), Some(Type::Code)))));
            analyze(&ast, &mut state);
            errors.extend(state.errors.clone());
            (Some((state, semantic_tokens)), errors)
        }
        Err(e) => (None, vec![e]),
    }
}
