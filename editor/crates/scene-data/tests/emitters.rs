//! Placed particle emitters: the level object and the visual payload, each of its format's
//! one schema.
use scene_data::*;

fn id(value: u128) -> StableId {
    StableId::from_u128(value)
}

fn placement(object: u128, emitter: u128) -> ObjectRecord {
    ObjectRecord {
        id: id(object),
        parent: None,
        sibling_order: object as u32,
        name: "Sparks".into(),
        local: RigidTransform::default(),
        data: ObjectData::Emitter {
            emitter_id: id(emitter).0,
        },
    }
}

fn codes(scene: &SceneDocument) -> Vec<String> {
    validation_diagnostics(scene)
        .into_iter()
        .map(|problem| problem.code)
        .collect()
}

fn cooked(placement: u128, emitter: u128) -> CookedEmitter {
    CookedEmitter {
        id: id(placement),
        emitter_id: id(emitter),
        emitter_sha256: ContentHash([7; 32]),
        transform: RigidTransform {
            translation_m: [1.0, 2.5, -3.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    }
}

#[test]
fn a_level_of_the_one_schema_places_emitters_and_no_other_schema_is_read() {
    let mut scene = current_map();
    assert_eq!(scene.schema, SCHEMA_VERSION);
    assert!(!scene.places_emitters());

    // An emitter is an object of the level's one schema: placing it changes no schema.
    scene.objects.push(placement(900, 5));
    assert!(scene.places_emitters());
    assert_eq!(codes(&scene), [] as [&str; 0]);
    assert_eq!(scene.schema, SCHEMA_VERSION);
    scene.objects.pop();
    assert_eq!(codes(&scene), [] as [&str; 0]);

    // Every other schema is refused, the one before it and the ones after it alike.
    for other in [0, 2, 3] {
        scene.schema = other;
        assert_eq!(codes(&scene), ["schema.unsupported"], "{other}");
    }
}

#[test]
fn an_emitter_names_its_asset_by_a_stable_id_and_holds_nothing_of_its_content() {
    let mut scene = current_map();
    scene.objects.push(placement(900, 5));
    let written = serde_json::to_value(scene.objects.last().unwrap()).unwrap();
    assert_eq!(
        written["data"],
        serde_json::json!({ "kind": "Emitter", "emitter_id": id(5).0 })
    );
    // Read back as it was written, and nothing more is taken.
    let read: SceneDocument = serde_json::from_slice(&serde_json::to_vec(&scene).unwrap()).unwrap();
    assert_eq!(read, scene);
    let mut with_hash = serde_json::to_value(&scene).unwrap();
    with_hash["objects"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["data"]["sha256"] = "00".into();
    assert!(serde_json::from_value::<SceneDocument>(with_hash).is_err());

    // What is no stable ID is no emitter's.
    for bad in [
        "",
        "sparks",
        id(0).0.as_str(),
        "0123456789ABCDEF0123456789ABCDEF",
    ] {
        let ObjectData::Emitter { emitter_id } = &mut scene.objects.last_mut().unwrap().data else {
            unreachable!();
        };
        *emitter_id = bad.to_owned();
        assert_eq!(codes(&scene), ["emitter.id_invalid"], "{bad:?}");
    }
}

#[test]
fn a_payload_without_emitters_says_it_places_none() {
    let payload = VisualPayload {
        schema: COOK_SCHEMA,
        terrain: vec![],
        emitters: vec![],
    };
    assert_eq!(
        serde_json::to_string(&payload).unwrap(),
        r#"{"schema":1,"terrain":[],"emitters":[]}"#
    );
    assert_eq!(payload.validate(&[]), Ok(()));
    assert_eq!(
        serde_json::from_str::<VisualPayload>(r#"{"schema":1,"terrain":[],"emitters":[]}"#)
            .unwrap(),
        payload
    );
}

#[test]
fn a_payload_with_emitters_has_exactly_its_fields() {
    let payload = VisualPayload {
        schema: COOK_SCHEMA,
        terrain: vec![],
        emitters: vec![cooked(0x21, 0x5)],
    };
    assert_eq!(payload.validate(&[]), Ok(()));
    // Pinned: these are the bytes the game's client reads.
    let hash: Vec<String> = std::iter::repeat_n("7".to_owned(), 32).collect();
    let written = format!(
        concat!(
            r#"{{"schema":1,"terrain":[],"emitters":[{{"id":"{}","emitter_id":"{}","#,
            r#""emitter_sha256":[{}],"transform":{{"translation_m":[1.0,2.5,-3.0],"#,
            r#""rotation_xyzw":[0.0,0.0,0.0,1.0]}}}}]}}"#
        ),
        id(0x21).0,
        id(0x5).0,
        hash.join(",")
    );
    assert_eq!(serde_json::to_string(&payload).unwrap(), written);
    assert_eq!(
        serde_json::from_str::<VisualPayload>(&written).unwrap(),
        payload
    );

    let refused = |json: String| serde_json::from_str::<VisualPayload>(&json).is_err();
    assert!(
        refused(r#"{"schema":1,"terrain":[]}"#.into()),
        "no emitters field"
    );
    assert!(
        refused(written.replacen(r#""transform""#, r#""scale":1.0,"transform""#, 1)),
        "a field no emitter has"
    );
    assert!(
        refused(written.replacen(r#""emitter_id":"#, r#""emitter":"#, 1)),
        "a field by another name"
    );
    assert!(refused(
        r#"{"schema":1,"terrain":[],"emitters":[],"more":1}"#.into()
    ));
    // Read, and then refused for its schema: there is one.
    let other: VisualPayload =
        serde_json::from_str(&written.replacen(r#""schema":1"#, r#""schema":2"#, 1)).unwrap();
    assert_eq!(
        other.validate(&[]).unwrap_err().code,
        "payload.visual_invalid"
    );
}

#[test]
fn an_emitters_key_holds_a_list() {
    let refused = |json: &str| serde_json::from_str::<VisualPayload>(json).is_err();
    assert!(refused(r#"{"schema":1,"terrain":[],"emitters":null}"#));
    assert!(refused(r#"{"schema":1,"terrain":[],"emitters":{}}"#));
    assert!(refused(r#"{"schema":1,"terrain":[],"emitters":0}"#));
    assert!(!refused(r#"{"schema":1,"terrain":[],"emitters":[]}"#));
}

#[test]
fn a_payload_has_one_form_for_one_content() {
    let valid = VisualPayload {
        schema: COOK_SCHEMA,
        terrain: vec![],
        emitters: vec![cooked(0x21, 0x5), cooked(0x22, 0x5)],
    };
    assert_eq!(valid.validate(&[]), Ok(()));
    let code = |payload: &VisualPayload| payload.validate(&[]).unwrap_err().code;

    // No placement is valid as well; another schema is not, with placements or without.
    let mut empty = valid.clone();
    empty.emitters.clear();
    assert_eq!(empty.validate(&[]), Ok(()));
    for payload in [&valid, &empty] {
        let mut other = payload.clone();
        other.schema = COOK_SCHEMA + 1;
        assert_eq!(code(&other), "payload.visual_invalid");
    }

    // Placements by ID, each once.
    let mut unordered = valid.clone();
    unordered.emitters.reverse();
    assert_eq!(code(&unordered), "payload.emitter_invalid");
    let mut twice = valid.clone();
    twice.emitters[1].id = id(0x21);
    assert_eq!(code(&twice), "payload.emitter_invalid");

    // What is no ID, and a place that is none.
    let mut nameless = valid.clone();
    nameless.emitters[0].emitter_id = StableId("sparks".into());
    assert_eq!(code(&nameless), "payload.emitter_invalid");
    let mut turned = valid.clone();
    turned.emitters[0].transform.rotation_xyzw = [0.0, 0.0, 0.0, 2.0];
    assert_eq!(code(&turned), "payload.emitter_invalid");
    let mut nowhere = valid.clone();
    nowhere.emitters[0].transform.translation_m[1] = f32::NAN;
    assert_eq!(code(&nowhere), "payload.emitter_invalid");

    let mut many = valid;
    many.emitters = (1..=MAX_EMITTERS as u128 + 1)
        .map(|n| cooked(n, 0x5))
        .collect();
    assert_eq!(code(&many), "payload.emitter_limit");
}
