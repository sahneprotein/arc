//! External Play control, independent of the authoring worker and UI.
mod api;
mod model;
pub use api::*;

mod backend;
mod handle;
mod supervisor;
pub use backend::*;
pub use handle::PlayHandle;

#[cfg(test)]
mod tests;

mod process;
mod wire;

mod preflight;
pub use preflight::check_project_play_inputs;

#[cfg(test)]
mod process_tests;
