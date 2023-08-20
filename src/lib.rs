pub mod addon;
pub mod analyze;
pub mod definition;
pub mod hover;
pub mod semantic_token;

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::Url;

    use super::*;

    #[test]
    fn bla() {
        let uri = Url::parse(
            "file:///home/azureuser/projects/arma/antistasi/Antistasi/fnc_AAFattackScore.sqf",
        )
        .unwrap();
        let (addon_path, functions) =
            if let Some((path, functions)) = addon::identify(&uri, "config.cpp") {
                (path, functions)
            } else if let Some((path, functions)) = addon::identify(&uri, "description.ext") {
                (path, functions)
            } else {
                panic!();
            };

        println!("{functions:#?}");
        println!("processing");
        let (state, _) = addon::process(addon_path, &functions);
    }
}
