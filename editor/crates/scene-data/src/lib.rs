//! Engine-free authored scene and CPU runtime data. No renderer/game/wire types.
mod fixture;
pub mod layers;
mod model;
pub mod rules;
mod runtime;
pub mod terrain;
mod validation;
pub use fixture::current_map;
pub use model::*;
pub use runtime::*;
pub use validation::{validate, validation_diagnostics, MAX_OBJECTS};

use sha2::{Digest, Sha256};

/// Semantic save identity: order-independent object storage, order-preserving IDs.
pub fn source_hash(scene: &SceneDocument) -> Result<ContentHash, Diagnostic> {
    validate(scene)?;
    let mut canonical = scene.clone();
    canonical.objects.sort_by(|a, b| a.id.cmp(&b.id));
    for object in &mut canonical.objects {
        if let ObjectData::Terrain(t) = &mut object.data {
            t.tiles.sort_by(|a, b| a.id.cmp(&b.id));
            for tile in &mut t.tiles {
                tile.layers.sort_by(|a, b| a.layer.cmp(&b.layer));
            }
            t.samples.sort_by_key(|s| s.key);
            t.layers.sort_by(|a, b| a.id.cmp(&b.id));
            t.rules.sort_by(|a, b| a.id.cmp(&b.id));
            for rule in &mut t.rules {
                rule.input_layers.sort();
                rule.input_rules.sort();
                rule.exclusion_layers.sort();
            }
        }
    }
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|e| Diagnostic::new("source.encode").with("detail", e.to_string()))?;
    Ok(ContentHash(Sha256::digest(bytes).into()))
}
