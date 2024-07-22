pub mod addon;
pub mod analyze;
mod completion;
pub mod definition;
pub mod hover;
pub mod semantic_token;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn bla() {
        let path: PathBuf = "./example.sqf".into();

        let content = std::fs::read_to_string(&path).unwrap();
        let configuration = sqf::analyzer::Configuration {
            file_path: path.into(),
            base_path: "".into(),
            ..Default::default()
        };

        let (state_semantic_completion, errors) =
            match analyze::compute(&content, configuration, Default::default()) {
                Ok((state, semantic, completion, errors)) => {
                    (Some((state, semantic, completion)), errors)
                }
                Err(e) => (None, vec![e]),
            };

        let (state, semantic_tokens, _) = state_semantic_completion.unwrap();

        assert_eq!(errors.len(), 1);
        assert_eq!(state.explanations.len(), 4);
        assert_eq!(semantic_tokens.len(), 33);
    }
}
