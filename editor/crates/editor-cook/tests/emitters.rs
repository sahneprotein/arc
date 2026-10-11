//! Placed particle emitters in the cook: only in the visual payload, with the hash the cook
//! is given for each, and never passed over.
use scene_data::*;

fn id(n: u128) -> StableId {
    StableId::from_u128(n)
}
fn placed(scene: &mut SceneDocument, object: u128, parent: Option<u128>, emitter: u128) {
    scene.objects.push(ObjectRecord {
        id: id(object),
        parent: parent.map(id),
        sibling_order: object as u32,
        name: "Sparks".into(),
        local: RigidTransform {
            translation_m: [1.0, 2.0, 3.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        data: ObjectData::Emitter {
            emitter_id: id(emitter).0,
        },
    });
    scene.upgrade_for_content();
}
/// The hash a test's emitter asset has: its number in every byte.
fn hash_of(emitter_id: &str) -> Result<ContentHash, Diagnostic> {
    let number = u8::from_str_radix(&emitter_id[30..], 16).unwrap();
    if number == 0xee {
        return Err(Diagnostic::new("emitter.asset_missing"));
    }
    Ok(ContentHash([number; 32]))
}
fn cook(scene: &SceneDocument) -> Result<editor_cook::CookedPackage, Diagnostic> {
    editor_cook::cook_with_emitters(scene, &hash_of, &|| Ok(()))
}

#[test]
fn a_level_without_placements_is_cooked_byte_for_byte_as_it_always_was() {
    let scene = current_map();
    let before = editor_cook::cook(&scene).unwrap();
    let with_assets = cook(&scene).unwrap();
    assert_eq!(with_assets.visual_bytes, before.visual_bytes);
    assert_eq!(with_assets.visual_hash, before.visual_hash);
    assert_eq!(with_assets.simulation_bytes, before.simulation_bytes);
    assert_eq!(before.visual.schema, COOK_SCHEMA);
    assert!(before.visual.emitters.is_empty());
    assert!(!String::from_utf8(before.visual_bytes)
        .unwrap()
        .contains("emitters"));

    // A level that once placed an emitter is of the second source schema; its cooked
    // payload is still the first, since it places none.
    let mut once = current_map();
    placed(&mut once, 900, None, 5);
    once.objects.pop();
    assert_eq!(once.schema, SCHEMA_VERSION_EMITTERS);
    let cooked = cook(&once).unwrap();
    assert_eq!(cooked.visual.schema, COOK_SCHEMA);
    assert_eq!(cooked.visual_bytes, with_assets.visual_bytes);
}

#[test]
fn placements_go_into_the_visual_payload_and_nowhere_else() {
    let plain = cook(&current_map()).unwrap();
    let mut scene = current_map();
    // A group at (10, 0, 0) with one placement under it, and one at the top.
    scene.objects.push(ObjectRecord {
        id: id(800),
        parent: None,
        sibling_order: 800,
        name: "Group".into(),
        local: RigidTransform {
            translation_m: [10.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        data: ObjectData::Group,
    });
    placed(&mut scene, 902, Some(800), 6);
    placed(&mut scene, 901, None, 5);
    let cooked = cook(&scene).unwrap();

    assert_eq!(cooked.visual.schema, VISUAL_SCHEMA_EMITTERS);
    assert_eq!(
        cooked.visual.emitters,
        [
            CookedEmitter {
                id: id(901),
                emitter_id: id(5),
                emitter_sha256: ContentHash([5; 32]),
                transform: RigidTransform {
                    translation_m: [1.0, 2.0, 3.0],
                    rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                },
            },
            // Through its parent: where it is in the world.
            CookedEmitter {
                id: id(902),
                emitter_id: id(6),
                emitter_sha256: ContentHash([6; 32]),
                transform: RigidTransform {
                    translation_m: [11.0, 2.0, 3.0],
                    rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                },
            },
        ]
    );
    // The server's payload does not know them, and the package's own schema is the first.
    assert_eq!(cooked.simulation_bytes, plain.simulation_bytes);
    assert_eq!(cooked.simulation.schema, COOK_SCHEMA);
    assert_ne!(cooked.visual_hash, plain.visual_hash);
    // The bytes are read back as the same payload, and are the same for the same level.
    let read: VisualPayload = serde_json::from_slice(&cooked.visual_bytes).unwrap();
    assert_eq!(read, cooked.visual);
    assert_eq!(cook(&scene).unwrap().visual_bytes, cooked.visual_bytes);
}

#[test]
fn a_placement_whose_emitter_is_refused_fails_the_cook_and_is_named() {
    let mut scene = current_map();
    placed(&mut scene, 901, None, 5);
    placed(&mut scene, 902, None, 0xee);
    let refused = cook(&scene).unwrap_err();
    assert_eq!(refused.code, "emitter.asset_missing");
    assert_eq!(refused.object, Some(id(902)));
    assert_eq!(refused.parameters["emitter_id"], id(0xee).0);

    // Without anyone to ask, a level that places an emitter is not cooked at all.
    scene.objects.pop();
    let no_game = editor_cook::cook(&scene).unwrap_err();
    assert_eq!(no_game.code, "emitter.no_game");
    assert_eq!(no_game.object, Some(id(901)));
}
