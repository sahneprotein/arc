//! The design tokens as a Feathers theme.
//!
//! Feathers looks colours up by token name at run time. The theme starts from Feathers' own dark
//! theme, whose palette is the source of the design system's neutral colours, and adds every
//! design token under `arc.<name>` so that the editor's widgets can use the additions.

use bevy::{
    color::Color,
    feathers::{
        dark_theme::create_dark_theme,
        theme::{ThemeToken, UiTheme},
    },
};

use super::{theme_key, TokenColor, COLORS};

/// A design colour as the engine's colour type, in the token's own colour space.
pub fn to_color(color: TokenColor) -> Color {
    match color {
        TokenColor::Oklch { l, c, h, a } => Color::oklcha(l, c, h, a),
        TokenColor::Srgb8 { r, g, b, a } => Color::srgba_u8(r, g, b, a),
    }
}

/// The editor's theme.
pub fn ui_theme() -> UiTheme {
    let mut props = create_dark_theme();
    for (name, color) in COLORS {
        props.color.insert(
            ThemeToken::new(format!("arc.{name}").into()),
            to_color(*color),
        );
    }
    UiTheme(props)
}

/// Background of the selected row or tile.
pub const SELECTION: ThemeToken = ThemeToken::new_static(theme_key::SELECTION);
/// Warning text: unsaved changes, cook needed.
pub const SIGNAL_WARN: ThemeToken = ThemeToken::new_static(theme_key::SIGNAL_WARN);
/// Error text.
pub const SIGNAL_ERROR: ThemeToken = ThemeToken::new_static(theme_key::SIGNAL_ERROR);
/// Behind the 3D view.
pub const SURFACE_SUNKEN: ThemeToken = ThemeToken::new_static(theme_key::SURFACE_SUNKEN);
/// Between the window and a dialog.
pub const SCRIM: ThemeToken = ThemeToken::new_static(theme_key::SCRIM);
/// Hairlines between panels and around fields.
pub const BORDER: ThemeToken = ThemeToken::new_static(theme_key::BORDER);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_design_colour_is_in_the_toolkit_theme_under_its_arc_name() {
        let theme = ui_theme();
        for (name, color) in COLORS {
            let token = ThemeToken::new(format!("arc.{name}").into());
            assert_eq!(theme.0.color.get(&token), Some(&to_color(*color)), "{name}");
        }
    }

    #[test]
    fn the_named_tokens_point_at_colours_the_theme_holds() {
        let theme = ui_theme();
        for token in [
            SELECTION,
            SIGNAL_WARN,
            SIGNAL_ERROR,
            SURFACE_SUNKEN,
            SCRIM,
            BORDER,
        ] {
            assert!(theme.0.color.contains_key(&token), "{token}");
        }
    }
}
