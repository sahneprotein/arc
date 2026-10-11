use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The schema of a level that places no particle emitter: what every level was.
pub const SCHEMA_VERSION: u32 = 1;
/// The schema of a level that places a particle emitter, or once did. A level goes to it
/// with its first placement and stays there: a level that had placements does not go back
/// when the last one is removed, so that its schema does not change back and forth.
pub const SCHEMA_VERSION_EMITTERS: u32 = 2;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableId(pub String);
impl StableId {
    pub fn from_u128(value: u128) -> Self {
        Self(format!("{value:032x}"))
    }
    pub fn valid(&self) -> bool {
        self.0.len() == 32
            && self
                .0
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && self.0 != "00000000000000000000000000000000"
    }
}
pub type ObjectId = StableId;
pub type TileId = StableId;
pub type LayerId = StableId;
pub type RuleId = StableId;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);
impl ContentHash {
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidTransform {
    pub translation_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
}
impl Default for RigidTransform {
    fn default() -> Self {
        Self {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ObjectData {
    Group,
    Box {
        half_extents_m: [f32; 3],
    },
    Spawn {
        order: u32,
    },
    Terrain(crate::terrain::TerrainDocument),
    /// A reusable particle emitter of the game, placed here: only what is seen, nothing
    /// the server simulates. The level names the emitter by its stable asset ID and holds
    /// nothing of its content; what the emitter is, and its exact bytes, are read from the
    /// game's folder when the level is cooked. Only in a level of schema
    /// [`SCHEMA_VERSION_EMITTERS`].
    Emitter {
        emitter_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRecord {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub sibling_order: u32,
    pub name: String,
    pub local: RigidTransform,
    pub data: ObjectData,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDocument {
    pub schema: u32,
    pub id: StableId,
    pub objects: Vec<ObjectRecord>,
}
impl SceneDocument {
    /// Whether the level places a particle emitter.
    pub fn places_emitters(&self) -> bool {
        self.objects
            .iter()
            .any(|o| matches!(o.data, ObjectData::Emitter { .. }))
    }
    /// Takes the level to the schema its content needs: to [`SCHEMA_VERSION_EMITTERS`]
    /// if it places an emitter. Never back.
    pub fn upgrade_for_content(&mut self) {
        if self.schema == SCHEMA_VERSION && self.places_emitters() {
            self.schema = SCHEMA_VERSION_EMITTERS;
        }
    }
    pub fn object(&self, id: &ObjectId) -> Option<&ObjectRecord> {
        self.objects.iter().find(|o| &o.id == id)
    }
    pub fn object_mut(&mut self, id: &ObjectId) -> Option<&mut ObjectRecord> {
        self.objects.iter_mut().find(|o| &o.id == id)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub parameters: BTreeMap<String, String>,
    pub object: Option<ObjectId>,
}
impl Diagnostic {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.into(),
            parameters: BTreeMap::new(),
            object: None,
        }
    }
    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        self.parameters.insert(key.into(), value.to_string());
        self
    }
    pub fn at(mut self, object: &ObjectId) -> Self {
        self.object = Some(object.clone());
        self
    }
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {:?}", self.code, self.parameters)
    }
}
impl std::error::Error for Diagnostic {}
