//! Canonical captured conversion of frozen arc-game shared/src/level.rs.
//! Original SHA256: 5bfbe0aae57dfb618ab48420bcc425b940c34d97db3524240cead4ed526feb93.
//! Tests compare against an independent source extract of its real map/spawn functions.
use crate::SceneDocument;
use std::sync::OnceLock;

pub fn current_map() -> SceneDocument {
    static MAP: OnceLock<SceneDocument> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../fixtures/current-map.arcscene"))
            .expect("checked-in current-map fixture must be valid JSON")
    })
    .clone()
}
