//! The level a new project starts with: a small arena for two teams.
//!
//! It is made for this purpose and is not a copy of any map of the game. A square floor of
//! 48 metres with a wall around it, raised ground in the middle that is walked onto over a
//! ramp from the east and one from the west, cover between that and the two ends, and four
//! spawn points at the north end and four at the south end, each row behind a wall of its
//! own. Turned half round about the middle the arena is the same, so neither end is the
//! better one.
//!
//! Only boxes, groups and spawn points: what every game of this engine plays, with nothing
//! that has to be in the project's folder besides the level. The spawn points are numbered
//! north, south, north, south, so that two clients of one run start at opposite ends.
use crate::{ObjectData, ObjectRecord, RigidTransform, SceneDocument, StableId, SCHEMA_VERSION};

/// Half the floor's side, in metres.
const HALF_FLOOR_M: f32 = 24.0;
/// How high the raised ground in the middle is, in metres.
const HIGH_GROUND_M: f32 = 1.5;
/// How far above the floor a spawn point is, in metres: as the editor puts a new one.
const SPAWN_HEIGHT_M: f32 = 2.0;
/// Where the spawn points of a row are, east to west, and how far from the middle the
/// rows are.
const SPAWN_XS_M: [f32; 4] = [-4.5, -1.5, 1.5, 4.5];
const SPAWN_ROW_M: f32 = 19.0;

/// A ramp from the floor, 10 metres from the middle, up to the raised ground's edge 4
/// metres from it: 6 metres along and 1.5 up, a quarter of a metre thick. These are the
/// half length of such a box, where its middle is, and the turn about the axis along it,
/// written out so that the level is the same bytes wherever it is made.
const RAMP_HALF_LENGTH_M: f32 = 3.092_329_3;
const RAMP_CENTRE_X_M: f32 = 6.939_366;
const RAMP_CENTRE_Y_M: f32 = 0.507_464_35;
const RAMP_TURN_Z: f32 = 0.122_183_26;
const RAMP_TURN_W: f32 = 0.992_507_6;

/// The level a new project starts with, under this ID. The ID is the level's own: a new
/// project is given one no other level has.
pub fn starter_level(id: StableId) -> SceneDocument {
    let mut made = Made::default();

    let shell = made.group("Arena");
    made.block(
        shell,
        "Floor",
        [0.0, -0.5, 0.0],
        [HALF_FLOOR_M, 0.5, HALF_FLOOR_M],
    );
    let (along, across) = (HALF_FLOOR_M + 1.0, HALF_FLOOR_M + 0.5);
    made.block(shell, "North wall", [0.0, 2.0, across], [along, 2.0, 0.5]);
    made.block(shell, "South wall", [0.0, 2.0, -across], [along, 2.0, 0.5]);
    made.block(
        shell,
        "East wall",
        [across, 2.0, 0.0],
        [0.5, 2.0, HALF_FLOOR_M],
    );
    made.block(
        shell,
        "West wall",
        [-across, 2.0, 0.0],
        [0.5, 2.0, HALF_FLOOR_M],
    );

    let high = made.group("High ground");
    let half_height = HIGH_GROUND_M / 2.0;
    made.block(
        high,
        "Platform",
        [0.0, half_height, 0.0],
        [4.0, half_height, 4.0],
    );
    for (name, side) in [("East ramp", 1.0), ("West ramp", -1.0)] {
        made.object(
            Some(high),
            name,
            RigidTransform {
                translation_m: [side * RAMP_CENTRE_X_M, RAMP_CENTRE_Y_M, 0.0],
                // It rises towards the middle: turned one way in the east, the other in
                // the west.
                rotation_xyzw: [0.0, 0.0, -side * RAMP_TURN_Z, RAMP_TURN_W],
            },
            ObjectData::Box {
                half_extents_m: [RAMP_HALF_LENGTH_M, 0.25, 2.0],
            },
        );
    }
    made.block(
        high,
        "Platform cover",
        [0.0, HIGH_GROUND_M + 0.5, 0.0],
        [1.5, 0.5, 0.3],
    );

    let cover = made.group("Cover");
    made.block(
        cover,
        "North spawn wall",
        [0.0, 1.25, 14.0],
        [5.0, 1.25, 0.4],
    );
    made.block(
        cover,
        "South spawn wall",
        [0.0, 1.25, -14.0],
        [5.0, 1.25, 0.4],
    );
    for (name, x, z) in [
        ("North-east", 1.0, 1.0),
        ("North-west", -1.0, 1.0),
        ("South-east", 1.0, -1.0),
        ("South-west", -1.0, -1.0),
    ] {
        made.block(
            cover,
            &format!("{name} pillar"),
            [x * 16.0, 2.0, z * 8.0],
            [0.75, 2.0, 0.75],
        );
        made.block(
            cover,
            &format!("{name} crate"),
            [x * 11.0, 0.6, z * 9.0],
            [1.0, 0.6, 1.0],
        );
    }

    for (name, z, first_order) in [("North", SPAWN_ROW_M, 0), ("South", -SPAWN_ROW_M, 1)] {
        let row = made.group(&format!("{name} spawns"));
        for (place, x) in (0u32..).zip(SPAWN_XS_M) {
            made.object(
                Some(row),
                &format!("{name} spawn {}", place + 1),
                at([x, SPAWN_HEIGHT_M, z]),
                ObjectData::Spawn {
                    order: first_order + 2 * place,
                },
            );
        }
    }

    SceneDocument {
        schema: SCHEMA_VERSION,
        id,
        objects: made.objects,
    }
}

fn at(translation_m: [f32; 3]) -> RigidTransform {
    RigidTransform {
        translation_m,
        ..RigidTransform::default()
    }
}

/// The objects as they are made, each with the next ID and the next place among the
/// objects of its parent.
#[derive(Default)]
struct Made {
    objects: Vec<ObjectRecord>,
}

impl Made {
    fn object(
        &mut self,
        parent: Option<u128>,
        name: &str,
        local: RigidTransform,
        data: ObjectData,
    ) -> u128 {
        let number = self.objects.len() as u128 + 1;
        let parent = parent.map(StableId::from_u128);
        let sibling_order = self
            .objects
            .iter()
            .filter(|object| object.parent == parent)
            .count() as u32;
        self.objects.push(ObjectRecord {
            id: StableId::from_u128(number),
            parent,
            sibling_order,
            name: name.to_owned(),
            local,
            data,
        });
        number
    }

    fn group(&mut self, name: &str) -> u128 {
        self.object(None, name, RigidTransform::default(), ObjectData::Group)
    }

    fn block(&mut self, parent: u128, name: &str, centre_m: [f32; 3], half_extents_m: [f32; 3]) {
        self.object(
            Some(parent),
            name,
            at(centre_m),
            ObjectData::Box { half_extents_m },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{current_map, source_hash, validate};

    fn starter() -> SceneDocument {
        starter_level(StableId::from_u128(0x5747_4152))
    }

    #[test]
    fn the_starter_is_a_level_of_its_own_under_the_id_it_is_given() {
        let level = starter();
        validate(&level).unwrap();
        assert_eq!(level.id, StableId::from_u128(0x5747_4152));
        assert_eq!(level.schema, SCHEMA_VERSION);
        // Not the game's old map under another ID.
        let mut sample = current_map();
        sample.id = level.id.clone();
        assert_ne!(level.objects, sample.objects);
        assert!(!level.objects.iter().any(|object| {
            sample
                .objects
                .iter()
                .any(|old| old.name == object.name && old.local == object.local)
        }));
        // The same level every time, and another one under another ID.
        assert_eq!(
            source_hash(&level).unwrap(),
            source_hash(&starter()).unwrap()
        );
        let other = starter_level(StableId::from_u128(7));
        assert_ne!(source_hash(&level).unwrap(), source_hash(&other).unwrap());
        assert_eq!(level.objects, other.objects);
    }

    #[test]
    fn two_rows_of_four_spawn_points_are_numbered_from_end_to_end_in_turn() {
        let level = starter();
        let mut spawns: Vec<(u32, f32, f32)> = level
            .objects
            .iter()
            .filter_map(|object| match object.data {
                ObjectData::Spawn { order } => Some((
                    order,
                    object.local.translation_m[2],
                    object.local.translation_m[1],
                )),
                _ => None,
            })
            .collect();
        spawns.sort_by_key(|spawn| spawn.0);
        assert_eq!(spawns.len(), 8);
        for (order, (number, z, height)) in spawns.into_iter().enumerate() {
            assert_eq!(number as usize, order);
            let north = order % 2 == 0;
            assert_eq!(z, if north { SPAWN_ROW_M } else { -SPAWN_ROW_M });
            assert_eq!(height, SPAWN_HEIGHT_M);
        }
    }

    #[test]
    fn turned_half_round_about_the_middle_the_arena_is_the_same() {
        let level = starter();
        let solid: Vec<_> = level
            .objects
            .iter()
            .filter(|object| !matches!(object.data, ObjectData::Group))
            .collect();
        for object in &solid {
            let [x, y, z] = object.local.translation_m;
            let [qx, qy, qz, qw] = object.local.rotation_xyzw;
            let kind = std::mem::discriminant(&object.data);
            let extents = |data: &ObjectData| match data {
                ObjectData::Box { half_extents_m } => Some(*half_extents_m),
                _ => None,
            };
            // Half a turn about the upright axis takes a turn about the axis along the
            // ramps to the opposite turn.
            let twin = solid.iter().any(|other| {
                other.local.translation_m == [-x, y, -z]
                    && other.local.rotation_xyzw == [qx, qy, -qz, qw]
                    && std::mem::discriminant(&other.data) == kind
                    && extents(&other.data) == extents(&object.data)
            });
            assert!(twin, "{} has no twin", object.name);
        }
    }

    #[test]
    fn each_ramp_leads_from_the_floor_to_the_edge_of_the_raised_ground() {
        let level = starter();
        for (name, side) in [("East ramp", 1.0f64), ("West ramp", -1.0)] {
            let ramp = level
                .objects
                .iter()
                .find(|object| object.name == name)
                .unwrap();
            let ObjectData::Box { half_extents_m } = ramp.data else {
                panic!("{name} is not a box");
            };
            let [cx, cy, _] = ramp.local.translation_m.map(f64::from);
            let [_, _, qz, qw] = ramp.local.rotation_xyzw.map(f64::from);
            let angle = 2.0 * qz.atan2(qw);
            let (half_length, half_thickness) =
                (f64::from(half_extents_m[0]), f64::from(half_extents_m[1]));
            // The two ends of the middle line of the ramp's upper face.
            let end = |along: f64| {
                (
                    cx + along * angle.cos() - half_thickness * angle.sin(),
                    cy + along * angle.sin() + half_thickness * angle.cos(),
                )
            };
            let mut ends = [end(-half_length), end(half_length)];
            ends.sort_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
            let [(top_x, top_y), (foot_x, foot_y)] = ends;
            assert!((top_x - side * 4.0).abs() < 1e-4, "{name}: {top_x}");
            assert!(
                (top_y - f64::from(HIGH_GROUND_M)).abs() < 1e-4,
                "{name}: {top_y}"
            );
            assert!((foot_x - side * 10.0).abs() < 1e-4, "{name}: {foot_x}");
            assert!(foot_y.abs() < 1e-4, "{name}: {foot_y}");
        }
    }
}
