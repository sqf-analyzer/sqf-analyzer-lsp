use std::collections::{HashMap, VecDeque};

use sqf::{
    analyzer::{BINARY, NULLARY, UNARY},
    preprocessor::Ast,
    span::{Span, Spanned},
};
use tower_lsp::lsp_types::SemanticTokenType;

#[derive(Debug)]
pub struct SemanticTokenLocation {
    pub start: usize,
    pub length: usize,
    pub token_type: usize,
}

pub const LEGEND_TYPE: &[SemanticTokenType] = &[
    SemanticTokenType::FUNCTION,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::STRING,
    SemanticTokenType::COMMENT,
    SemanticTokenType::NUMBER,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::MACRO,
];

lazy_static::lazy_static! {
    static ref MAP: HashMap<SemanticTokenType, usize> = {
        LEGEND_TYPE.iter().enumerate().map(|(i, l)| (l.clone(), i)).collect()
    };
}

fn to_st(span: Span, token_type: SemanticTokenType) -> SemanticTokenLocation {
    SemanticTokenLocation {
        start: span.0,
        length: span.1 - span.0,
        token_type: MAP[&token_type],
    }
}

fn recurse(ast: &Ast, container: &mut Vec<SemanticTokenLocation>) {
    match ast {
        Ast::Ifdef(ifdef) | Ast::Ifndef(ifdef) => {
            container.push(to_st(ifdef.keyword.span, SemanticTokenType::MACRO));
            if let Some(else_) = ifdef.else_keyword {
                container.push(to_st(else_.span, SemanticTokenType::MACRO));
            }
        }
        Ast::If(if_) => {
            container.push(to_st(if_.keyword.span, SemanticTokenType::MACRO));
        }
        Ast::Define(define) => {
            container.push(to_st(define.keyword.span, SemanticTokenType::MACRO));
        }
        Ast::Undefine(keyword, variable) => {
            container.push(to_st(keyword.span, SemanticTokenType::MACRO));
            container.push(to_st(variable.span, SemanticTokenType::VARIABLE));
        }
        Ast::Include(keyword, token) => {
            container.push(to_st(keyword.span, SemanticTokenType::MACRO));
            container.push(to_st(token.span, SemanticTokenType::STRING));
        }
        Ast::Comment(token) => container.push(to_st(token.span, SemanticTokenType::COMMENT)),
        Ast::Term(token) => {
            if let Some(t) = get_semantic_tokens(token) {
                container.push(t)
            }
        }
    }
}

fn get_semantic_tokens(token: &Spanned<&str>) -> Option<SemanticTokenLocation> {
    let span = token.span;

    let bytes = token.inner.as_bytes();
    if bytes.len() >= 2 && (bytes[0] == bytes[bytes.len() - 1]) && bytes[0] == b'\"' {
        return Some(to_st(span, SemanticTokenType::STRING));
    }

    if token.inner.parse::<f32>().is_ok() {
        return Some(to_st(span, SemanticTokenType::NUMBER));
    }

    let token = token.inner.to_lowercase(); // SQF is case insensitive
    let token = token.as_str();
    if BINARY.contains_key(token) || UNARY.contains_key(token) || NULLARY.contains_key(token) {
        Some(to_st(span, SemanticTokenType::KEYWORD))
    } else {
        Some(to_st(span, SemanticTokenType::VARIABLE))
    }
}

pub fn semantic_tokens(tokens: &VecDeque<Ast>) -> Vec<SemanticTokenLocation> {
    let mut container = vec![];

    for ast in tokens {
        recurse(ast, &mut container);
    }
    container
}
