//! Reading values out of the design token file.
//!
//! Compiled twice: by `build.rs` (through `#[path]`), which generates the theme, and by this
//! crate, whose tests check it. So it uses nothing but `std`.

/// A colour as the token file writes it. Kept in its own colour space so that the values the
/// toolkit receives are exactly the ones the design system holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenColor {
    /// `oklch(L C H)` or `oklch(L C H / A)`: lightness 0..1, chroma, hue in degrees, alpha 0..1.
    Oklch { l: f32, c: f32, h: f32, a: f32 },
    /// `#rrggbb` or `#rrggbbaa`: non-linear sRGB, 0..255 each.
    Srgb8 { r: u8, g: u8, b: u8, a: u8 },
}

/// Parses a colour value of the token file. `None` for anything this theme does not support,
/// which fails the build with the token's name.
pub fn parse_color(value: &str) -> Option<TokenColor> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        let digit = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        return match hex.len() {
            6 => Some(TokenColor::Srgb8 {
                r: digit(0)?,
                g: digit(2)?,
                b: digit(4)?,
                a: 255,
            }),
            8 => Some(TokenColor::Srgb8 {
                r: digit(0)?,
                g: digit(2)?,
                b: digit(4)?,
                a: digit(6)?,
            }),
            _ => None,
        };
    }
    let inner = value.strip_prefix("oklch(")?.strip_suffix(')')?;
    let (channels, alpha) = match inner.split_once('/') {
        Some((channels, alpha)) => (channels, alpha.trim().parse::<f32>().ok()?),
        None => (inner, 1.0),
    };
    let mut parts = channels.split_whitespace().map(str::parse::<f32>);
    let (l, c, h) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    if parts.next().is_some() {
        return None;
    }
    Some(TokenColor::Oklch { l, c, h, a: alpha })
}

/// Parses a length of the token file, `24px` or a bare number, into logical pixels.
pub fn parse_px(value: &str) -> Option<f32> {
    let value = value.trim();
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .trim()
        .parse()
        .ok()
}

/// The Rust constant name of a token: `surface-item-active` becomes `SURFACE_ITEM_ACTIVE`.
pub fn const_name(token: &str) -> String {
    token
        .chars()
        .map(|c| match c {
            'a'..='z' => c.to_ascii_uppercase(),
            'A'..='Z' | '0'..='9' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oklch_value_keeps_its_three_channels_and_is_opaque() {
        assert_eq!(
            parse_color("oklch(0.2866 0.0072 285.93)"),
            Some(TokenColor::Oklch {
                l: 0.2866,
                c: 0.0072,
                h: 285.93,
                a: 1.0
            })
        );
    }

    #[test]
    fn an_oklch_value_may_carry_an_alpha_after_a_slash() {
        assert_eq!(
            parse_color("oklch(0.5 0.1 255.4 / 0.5)"),
            Some(TokenColor::Oklch {
                l: 0.5,
                c: 0.1,
                h: 255.4,
                a: 0.5
            })
        );
    }

    #[test]
    fn a_hex_value_is_srgb_with_or_without_alpha() {
        assert_eq!(
            parse_color("#ff007d"),
            Some(TokenColor::Srgb8 {
                r: 255,
                g: 0,
                b: 125,
                a: 255
            })
        );
        assert_eq!(
            parse_color("#206ec980"),
            Some(TokenColor::Srgb8 {
                r: 0x20,
                g: 0x6e,
                b: 0xc9,
                a: 0x80
            })
        );
    }

    #[test]
    fn what_the_theme_cannot_read_is_refused_and_not_guessed() {
        for value in [
            "red",
            "#12345",
            "oklch(0.5 0.1)",
            "oklch(0.5 0.1 20 30)",
            "rgb(1 2 3)",
            "{accent}",
        ] {
            assert_eq!(parse_color(value), None, "{value}");
        }
    }

    #[test]
    fn a_length_is_logical_pixels_with_or_without_the_unit() {
        assert_eq!(parse_px("24px"), Some(24.0));
        assert_eq!(parse_px("6"), Some(6.0));
        assert_eq!(parse_px("1.5em"), None);
    }

    #[test]
    fn a_token_name_becomes_a_rust_constant_name() {
        assert_eq!(const_name("surface-item-active"), "SURFACE_ITEM_ACTIVE");
        assert_eq!(const_name("space-1"), "SPACE_1");
    }
}
