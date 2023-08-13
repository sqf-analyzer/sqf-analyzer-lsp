use sqf::{analyzer::State, span::Span};

fn in_span((start, end): Span, offset: usize) -> bool {
    offset >= start && offset < end
}

pub fn hover(state: &State, offset: usize) -> Option<&'static str> {
    state
        .explanations
        .iter()
        .find_map(move |(k, v)| in_span(*k, offset).then_some(*v))
}
