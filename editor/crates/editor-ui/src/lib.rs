//! The arc editor's native UI, on the engine's own UI and the Feathers widgets.
//!
//! - `theme/`: colours, sizes, radii and text styles, generated from the design tokens, and
//!   the same as a Feathers theme.
//! - `wording.rs`: every sentence the editor shows, from the core's codes and keys.
//! - `session.rs`: the UI's end of the editor core: snapshots in, commands out.
//! - `shell.rs`: the window's regions and the plugin.
//! - `rules.rs`: landscape rules as the panel handles them, without drawing.
//! - `particles.rs`: the game's particle emitters as the Particles tool handles them, and
//!   the preview's controls, without drawing.
//! - `tool.rs`: which tool the pointer has, and the landscape brush's settings.
//! - `play_script.rs`: a stand-in for the game's processes, for tests and scripted checks.
//! - `panels/`: toolbar and its menus, hierarchy, inspector, landscape tool, status bar, the
//!   notice of a problem, and the dialogs for levels and for unsaved changes.
//!
//! Each panel and widget is named after its page in the Claude Design system "Arc Editor".

/// The game's look profile as the editor's services read it, for the application's 3D view.
pub mod look {
    pub use editor_services::{
        load_look_profile, look_profile_path, parse_look_profile, procedural_brick_v1, LoadedLook,
        LookBoxes, LookProfile, LookTexture,
    };
}
pub mod panels;
pub mod particles;
pub mod play_script;
pub mod rules;
pub mod session;
pub mod shell;
pub mod theme;
pub mod tool;
pub mod wording;

pub use panels::closing::Shutdown;
pub use panels::levels::LevelDialog;
pub use panels::menu::MenuOpen;
pub use panels::unsaved::{Question, Unsaved};
pub use particles::{Particles, Preview};
pub use session::{Editor, PackageCheck};
pub use shell::{EditorUiPlugin, Modal, PanelChoice, ViewportSlot};
pub use tool::{
    Brush, BrushAim, BrushMode, ChosenRule, LookStatus, PaintLayer, PlaceAt, Snap, Tool, ViewMode,
};
