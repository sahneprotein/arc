//! UI-independent document actor, immutable views and validated command history.
mod brush;
mod command;
mod editing;
mod review;
pub use editing::*;
pub use review::*;
mod paint;
pub use paint::{terrain_paint, MAX_PAINT_CHANGED_TEXELS, MAX_PAINT_POINTS, MAX_PAINT_RADIUS_M};
mod sculpt;
mod session;
mod surface;
pub use surface::{terrain_surface_hit, TerrainSurfaceHit, TerrainSurfaceRay};
mod view;
pub use command::*;
pub use scene_data::{ContentHash, Diagnostic, ObjectId, SceneDocument, StableId};
pub use sculpt::{
    terrain_hit_height_m, terrain_sculpt, MAX_SCULPT_CHANGED_SAMPLES, MAX_SCULPT_POINTS,
    MAX_SCULPT_RADIUS_M,
};
pub use session::EditorSession;
pub use view::*;
