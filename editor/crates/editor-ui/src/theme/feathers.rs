//! The design tokens as a Feathers theme.
//!
//! Feathers looks colours up by token name at run time. The theme starts from Feathers' own dark
//! theme, whose palette is the source of the design system's neutral colours, and adds every
//! design token under `arc.<name>` so that the editor's widgets can use the additions. Where
//! the design system gives one of the toolkit's own widgets a colour that the toolkit's theme
//! does not have, the toolkit's token is set to the design's ([`TOOLKIT`]).

use bevy::{
    color::Color,
    feathers::{
        dark_theme::create_dark_theme,
        theme::{ThemeToken, UiTheme},
        tokens,
    },
};

use super::{color, theme_key, TokenColor, COLORS};

/// The toolkit's tokens the design system decides, each with the design token it takes.
///
/// The Menu page: `surface-pane` and `border` for the box, `surface-item` behind the row under
/// the pointer or with focus, `text-on-accent` for captions, `text-dim` for rows that are off.
/// The page has no colour for a row while it is pressed; that one stays the toolkit's.
pub const TOOLKIT: &[(ThemeToken, TokenColor)] = &[
    (tokens::MENU_BG, color::SURFACE_PANE),
    (tokens::MENU_BORDER, color::BORDER),
    (tokens::MENUITEM_BG_HOVER, color::SURFACE_ITEM),
    (tokens::MENUITEM_BG_FOCUSED, color::SURFACE_ITEM),
    (tokens::MENUITEM_TEXT, color::TEXT_ON_ACCENT),
    (tokens::MENUITEM_TEXT_DISABLED, color::TEXT_DIM),
];

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
    for (token, color) in TOOLKIT {
        props.color.insert(token.clone(), to_color(*color));
    }
    UiTheme(props)
}

/// Background of the selected row or tile.
pub const SELECTION: ThemeToken = ThemeToken::new_static(theme_key::SELECTION);
/// Warning text: unsaved changes, cook needed.
pub const SIGNAL_WARN: ThemeToken = ThemeToken::new_static(theme_key::SIGNAL_WARN);
/// Error text.
pub const SIGNAL_ERROR: ThemeToken = ThemeToken::new_static(theme_key::SIGNAL_ERROR);
/// A game that runs: the Play control's word and the status bar's.
pub const SIGNAL_PLAY: ThemeToken = ThemeToken::new_static(theme_key::SIGNAL_PLAY);
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
    fn the_menu_has_the_colours_its_design_page_names() {
        let theme = ui_theme();
        let toolkit = create_dark_theme();
        let design = |name: &str| to_color(crate::theme::color_by_name(name).unwrap());
        for (token, name) in [
            (tokens::MENU_BG, "surface-pane"),
            (tokens::MENU_BORDER, "border"),
            (tokens::MENUITEM_BG_HOVER, "surface-item"),
            (tokens::MENUITEM_BG_FOCUSED, "surface-item"),
            (tokens::MENUITEM_TEXT, "text-on-accent"),
            (tokens::MENUITEM_TEXT_DISABLED, "text-dim"),
        ] {
            assert_eq!(theme.0.color.get(&token), Some(&design(name)), "{token}");
        }
        // A pressed row has no colour in the design: it is the toolkit's own, untouched.
        assert_eq!(
            theme.0.color.get(&tokens::MENUITEM_BG_PRESSED),
            toolkit.color.get(&tokens::MENUITEM_BG_PRESSED)
        );
    }

    #[test]
    fn the_named_tokens_point_at_colours_the_theme_holds() {
        let theme = ui_theme();
        for token in [
            SELECTION,
            SIGNAL_WARN,
            SIGNAL_ERROR,
            SIGNAL_PLAY,
            SURFACE_SUNKEN,
            SCRIM,
            BORDER,
        ] {
            assert!(theme.0.color.contains_key(&token), "{token}");
        }
    }
}
