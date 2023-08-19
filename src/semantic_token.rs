use std::collections::{HashMap, VecDeque};

use sqf::{
    analyzer::{MissionNamespace, BINARY, NULLARY, UNARY},
    preprocessor::Ast,
    span::Span,
    UncasedStr,
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
    SemanticTokenType::TYPE,
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

fn recurse(ast: &Ast, container: &mut Vec<SemanticTokenLocation>, mission: &MissionNamespace) {
    match ast {
        Ast::Ifdef(ifdef) | Ast::Ifndef(ifdef) => {
            container.push(to_st(ifdef.keyword.span, SemanticTokenType::MACRO));
            container.push(to_st(ifdef.endif_keyword.span, SemanticTokenType::MACRO));
            if let Some(else_) = ifdef.else_keyword {
                container.push(to_st(else_.span, SemanticTokenType::MACRO));
            }
            for token in &ifdef.then {
                recurse(token, container, mission)
            }
            for token in &ifdef.else_ {
                recurse(token, container, mission)
            }
        }
        Ast::If(if_) => {
            container.push(to_st(if_.keyword.span, SemanticTokenType::MACRO));
            for token in &if_.expr {
                recurse(token, container, mission)
            }
            container.push(to_st(if_.endif_keyword.span, SemanticTokenType::MACRO));
            for token in &if_.then {
                recurse(token, container, mission)
            }
            for token in &if_.else_ {
                recurse(token, container, mission)
            }
        }
        Ast::Define(define) => {
            container.push(to_st(define.keyword.span, SemanticTokenType::MACRO));
            container.push(to_st(define.name.span, SemanticTokenType::VARIABLE));
            if let Some(tokens) = &define.arguments {
                for token in tokens {
                    container.push(to_st(token.span, infer_st(token.inner.as_ref(), mission)));
                }
            }
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
        Ast::Term(token) => container.push(to_st(token.span, infer_st(token.inner, mission))),
    }
}

fn infer_st(token: &str, mission: &MissionNamespace) -> SemanticTokenType {
    let bytes = token.as_bytes();
    if bytes.len() >= 2 && (bytes[0] == bytes[bytes.len() - 1]) && bytes[0] == b'\"' {
        return SemanticTokenType::STRING;
    }

    if token.parse::<f32>().is_ok() {
        return SemanticTokenType::NUMBER;
    }

    let token = UncasedStr::new(token);

    if BINARY.contains_key(token) || UNARY.contains_key(token) || NULLARY.contains_key(token) {
        SemanticTokenType::KEYWORD
    } else if let Some((_, Some(type_))) = mission.get(token) {
        match type_.type_() {
            sqf::types::Type::Code => SemanticTokenType::FUNCTION,
            _ => SemanticTokenType::VARIABLE,
        }
    } else {
        SemanticTokenType::VARIABLE
    }
}

pub fn semantic_tokens(
    tokens: &VecDeque<Ast>,
    mission: &MissionNamespace,
) -> Vec<SemanticTokenLocation> {
    let mut container = vec![];

    for ast in tokens {
        recurse(ast, &mut container, mission);
    }
    container.sort_by_key(|x| x.start);

    container
}
