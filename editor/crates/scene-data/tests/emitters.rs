//! Placed particle emitters: the level object, the level's second schema, and the visual
//! payload's second schema.
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
fn a_level_goes_to_its_second_schema_with_its_first_emitter_and_stays_there() {
    let mut scene = current_map();
    assert_eq!(scene.schema, SCHEMA_VERSION);
    assert!(!scene.places_emitters());
    scene.upgrade_for_content();
    assert_eq!(
        scene.schema, SCHEMA_VERSION,
        "nothing in it needs the second"
    );

    // An emitter in a level of the first schema is refused: a reader of that schema would
    // not know the object.
    scene.objects.push(placement(900, 5));
    assert_eq!(codes(&scene), ["schema.emitter_requires_v2"]);
    scene.upgrade_for_content();
    assert_eq!(scene.schema, SCHEMA_VERSION_EMITTERS);
    assert_eq!(codes(&scene), [] as [&str; 0]);

    // The last emitter removed: the level stays at the second schema and is valid there.
    scene.objects.pop();
    scene.upgrade_for_content();
    assert_eq!(scene.schema, SCHEMA_VERSION_EMITTERS);
    assert_eq!(codes(&scene), [] as [&str; 0]);
    // A level of the first schema is still read as it always was, and no other schema is.
    scene.schema = 3;
    assert_eq!(codes(&scene), ["schema.unsupported"]);
}

#[test]
fn an_emitter_names_its_asset_by_a_stable_id_and_holds_nothing_of_its_content() {
    let mut scene = current_map();
    scene.objects.push(placement(900, 5));
    scene.upgrade_for_content();
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
fn a_payload_without_emitters_is_written_as_it_always_was() {
    let payload = VisualPayload {
        schema: COOK_SCHEMA,
        terrain: vec![],
        emitters: vec![],
    };
    assert_eq!(
        serde_json::to_string(&payload).unwrap(),
        r#"{"schema":1,"terrain":[]}"#
    );
    assert_eq!(payload.validate(&[]), Ok(()));
    assert_eq!(
        serde_json::from_str::<VisualPayload>(r#"{"schema":1,"terrain":[]}"#).unwrap(),
        payload
    );
}

#[test]
fn a_payload_with_emitters_is_of_the_second_schema_and_has_exactly_its_fields() {
    let payload = VisualPayload {
        schema: VISUAL_SCHEMA_EMITTERS,
        terrain: vec![],
        emitters: vec![cooked(0x21, 0x5)],
    };
    assert_eq!(payload.validate(&[]), Ok(()));
    // Pinned: these are the bytes the game's client reads.
    let hash: Vec<String> = std::iter::repeat_n("7".to_owned(), 32).collect();
    let written = format!(
        concat!(
            r#"{{"schema":2,"terrain":[],"emitters":[{{"id":"{}","emitter_id":"{}","#,
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

    // Each schema has exactly its fields.
    let refused = |json: String| serde_json::from_str::<VisualPayload>(&json).is_err();
    assert!(
        refused(r#"{"schema":2,"terrain":[]}"#.into()),
        "no emitters field"
    );
    assert!(
        refused(written.replacen(r#""schema":2"#, r#""schema":1"#, 1)),
        "emitters in the first schema"
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
        r#"{"schema":2,"terrain":[],"emitters":[],"more":1}"#.into()
    ));
}

#[test]
fn an_emitters_key_that_holds_no_list_is_not_taken_for_no_key() {
    let refused = |json: &str| serde_json::from_str::<VisualPayload>(json).is_err();
    // The first schema has no such key, whatever the key holds.
    assert!(refused(r#"{"schema":1,"terrain":[],"emitters":null}"#));
    assert!(refused(r#"{"schema":1,"terrain":[],"emitters":[]}"#));
    // The second must have a list there.
    assert!(refused(r#"{"schema":2,"terrain":[],"emitters":null}"#));
    assert!(refused(r#"{"schema":2,"terrain":[],"emitters":{}}"#));
    assert!(refused(r#"{"schema":2,"terrain":[],"emitters":0}"#));
    // And each still reads what it always read.
    assert!(!refused(r#"{"schema":1,"terrain":[]}"#));
    let empty: VisualPayload =
        serde_json::from_str(r#"{"schema":2,"terrain":[],"emitters":[]}"#).unwrap();
    // Read, and then refused for what it holds: the second schema places something.
    assert_eq!(
        empty.validate(&[]).unwrap_err().code,
        "payload.visual_invalid"
    );
}

#[test]
fn a_payload_has_one_form_for_one_content() {
    let valid = VisualPayload {
        schema: VISUAL_SCHEMA_EMITTERS,
        terrain: vec![],
        emitters: vec![cooked(0x21, 0x5), cooked(0x22, 0x5)],
    };
    assert_eq!(valid.validate(&[]), Ok(()));
    let code = |payload: &VisualPayload| payload.validate(&[]).unwrap_err().code;

    // The second schema without a placement, and the first with one.
    let mut empty = valid.clone();
    empty.emitters.clear();
    assert_eq!(code(&empty), "payload.visual_invalid");
    let mut first = valid.clone();
    first.schema = COOK_SCHEMA;
    assert_eq!(code(&first), "payload.visual_invalid");

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
