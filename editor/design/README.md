# Editor design tokens

`tokens.json` is a copy of the tokens of the Claude Design system "Arc Editor". It is the one
source of the editor's colours, text sizes, spacing, radii and control sizes. The `editor-ui`
crate generates its theme from this file when it is built, so the file and the code cannot
drift apart.

To change the look: change the token in Claude Design, copy the system's `tokens.json` here,
and build. Do not edit colours or sizes in Rust.

The neutral palette, accent, axis colours, control sizes and text sizes are the values of the
engine's Feathers toolkit at this branch's base (`crates/bevy_feathers/src/palette.rs`,
`dark_theme.rs`, `constants.rs`). Tokens whose usage note says "Arc addition" are the editor's
own.

## Plans

`roadmap.md` is the order of the work on the editor and the game from 2026-10-25 on, with
what each milestone must show before it counts as done. `history/` keeps earlier plans as the
record of their time.
