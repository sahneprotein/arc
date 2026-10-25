use scene_data::*;
#[expect(
    clippy::disallowed_methods,
    reason = "independent frozen game oracle must retain its original trig arithmetic"
)]
#[path = "support/legacy_map.rs"]
mod legacy;

#[test]
fn current_map_matches_frozen_game_source_bit_for_bit() {
    let scene = current_map();
    validate(&scene).unwrap();
    let boxes: Vec<_> = scene
        .objects
        .iter()
        .filter_map(|o| match &o.data {
            ObjectData::Box { half_extents_m } => Some((
                o.local.translation_m,
                *half_extents_m,
                o.local.rotation_xyzw,
            )),
            _ => None,
        })
        .collect();
    let reference = legacy::test_level();
    assert_eq!(boxes.len(), 14);
    assert_eq!(boxes.len(), reference.len());
    for ((center, half, rotation), expected) in boxes.iter().zip(reference) {
        assert_eq!(
            center.map(f32::to_bits),
            expected.center.to_array().map(f32::to_bits)
        );
        assert_eq!(
            half.map(f32::to_bits),
            expected.half_extents.to_array().map(f32::to_bits)
        );
        assert_eq!(
            rotation.map(f32::to_bits),
            expected.rotation.to_array().map(f32::to_bits)
        );
    }
    let spawns: Vec<_> = scene
        .objects
        .iter()
        .filter(|o| matches!(o.data, ObjectData::Spawn { .. }))
        .collect();
    assert_eq!(spawns.len(), 8);
    for (i, o) in spawns.iter().enumerate() {
        assert_eq!(
            o.local.translation_m.map(f32::to_bits),
            legacy::spawn_point(i as u64).to_array().map(f32::to_bits)
        );
    }
    let encoded = serde_json::to_vec(&scene).unwrap();
    let decoded: SceneDocument = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, scene);
}
#[test]
fn malformed_identity_references_and_transforms_fail_with_codes() {
    let cases = [
        ("id.invalid_or_duplicate", 0),
        ("reference.missing_parent", 1),
        ("hierarchy.cycle", 2),
        ("transform.rotation_not_unit", 3),
        ("transform.nonfinite", 4),
        ("box.extents", 5),
    ];
    for (code, case) in cases {
        let mut scene = current_map();
        match case {
            0 => scene.objects[1].id = scene.objects[0].id.clone(),
            1 => scene.objects[0].parent = Some(StableId::from_u128(1000)),
            2 => scene.objects[0].parent = Some(scene.objects[0].id.clone()),
            3 => scene.objects[0].local.rotation_xyzw = [0.0; 4],
            4 => scene.objects[0].local.translation_m[0] = f32::NAN,
            5 => {
                scene.objects[0].data = ObjectData::Box {
                    half_extents_m: [-1.0, 1.0, 1.0],
                }
            }
            _ => unreachable!(),
        }
        assert_eq!(validate(&scene).unwrap_err().code, code);
    }
}
#[test]
fn object_storage_order_is_not_semantic_but_authored_names_are() {
    let mut scene = current_map();
    let hash = source_hash(&scene).unwrap();
    scene.objects.reverse();
    assert_eq!(source_hash(&scene).unwrap(), hash);
    scene.objects[0].name.push('!');
    assert_ne!(source_hash(&scene).unwrap(), hash);
    // The format has one version: any other is refused.
    scene.schema = scene_data::SCHEMA_VERSION + 1;
    assert_eq!(validate(&scene).unwrap_err().code, "schema.unsupported");
    let invalid = serde_json::to_vec(&serde_json::json!({"schema":1,"id":"00000000000000000000000000000a7c","objects":[],"unchecked":true})).unwrap();
    assert!(serde_json::from_slice::<SceneDocument>(&invalid).is_err());
}
