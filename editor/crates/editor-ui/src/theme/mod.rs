//! The editor's theme: every colour, size, radius and text style the UI may use.
//!
//! All values are generated from `editor/design/tokens.json` when the crate is built
//! (`build.rs`). Nothing here is written by hand, and UI code names a token instead of a
//! literal. To change a value, change the token in the design system.

pub mod feathers;
pub mod token_parse;

pub use token_parse::TokenColor;

/// Which typeface a text style uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFamily {
    /// The interface face (Fira Sans).
    Sans,
    /// The face for numbers, paths and hashes (Fira Mono).
    Mono,
}

/// A text style of the design system. Sizes are logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub family: TextFamily,
    pub size: f32,
    pub line_height: f32,
    pub weight: u64,
}

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));

/// Looks a colour token up by its design-system name.
pub fn color_by_name(name: &str) -> Option<TokenColor> {
    COLORS
        .iter()
        .find_map(|(token, color)| (*token == name).then_some(*color))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token file as the tests read it, independently of the build script.
    fn token_file() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../design/tokens.json"))
            .expect("the token file is JSON")
    }

    #[test]
    fn every_colour_of_the_token_file_is_in_the_theme_with_the_same_value() {
        let file = token_file();
        let theme = file["color"]["themes"][0]["id"].as_str().unwrap();
        let tokens = file["color"]["tokens"].as_array().unwrap();
        assert_eq!(tokens.len(), COLORS.len());
        for token in tokens {
            let name = token["name"].as_str().unwrap();
            let value = token["value"]
                .as_str()
                .or_else(|| token["value"][theme].as_str())
                .unwrap();
            assert_eq!(
                color_by_name(name),
                token_parse::parse_color(value),
                "{name}"
            );
        }
    }

    #[test]
    fn the_sizes_the_layout_rests_on_are_the_design_systems() {
        let file = token_file();
        let size = |name: &str| {
            file["size"]["tokens"]
                .as_array()
                .unwrap()
                .iter()
                .find(|token| token["name"] == name)
                .and_then(|token| token_parse::parse_px(token["value"].as_str()?))
                .unwrap()
        };
        assert_eq!(size::ROW_HEIGHT, size("row-height"));
        assert_eq!(size::HEADER_HEIGHT, size("header-height"));
        assert_eq!(size::STATUSBAR_HEIGHT, size("statusbar-height"));
    }

    #[test]
    fn text_styles_carry_the_family_size_and_weight_of_the_token_file() {
        let file = token_file();
        let style = |group: &str, name: &str| {
            file["type"]["groups"]
                .as_array()
                .unwrap()
                .iter()
                .find(|candidate| candidate["name"] == group)
                .and_then(|group| {
                    group["styles"]
                        .as_array()?
                        .iter()
                        .find(|style| style["name"] == name)
                        .cloned()
                })
                .unwrap()
        };
        let size = |style: &serde_json::Value| {
            token_parse::parse_px(style["fontSize"].as_str().unwrap()).unwrap()
        };
        assert_eq!(text::BODY.size, size(&style("Interface", "body")));
        assert_eq!(text::MICRO.size, size(&style("Interface", "micro")));
        assert_eq!(text::NUMBER.size, size(&style("Data", "number")));
        assert_eq!(text::NUMBER.family, TextFamily::Mono);
        assert_eq!(text::BODY.family, TextFamily::Sans);
        assert_eq!(
            Some(text::TITLE.weight),
            style("Interface", "title")["fontWeight"].as_u64()
        );
    }

    #[test]
    fn an_unknown_colour_name_is_not_a_colour() {
        assert_eq!(color_by_name("no-such-token"), None);
        assert!(color_by_name("accent").is_some());
    }
}
