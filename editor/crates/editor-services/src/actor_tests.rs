use super::*;
use std::{collections::BTreeSet, time::Duration};

#[test]
fn queue_full_does_not_supersede_last_accepted_and_every_ticket_finishes_once() {
    let mut handle = EditorHandle::start(scene_data::current_map()).unwrap();
    let mut tickets = vec![];
    loop {
        match handle.job(JobOperation::Geometry) {
            Ok(ticket) => tickets.push(ticket),
            Err(error) => {
                assert_eq!(error.code, "queue.full");
                break;
            }
        }
        // Event backpressure must stop the single worker and fill the64 request FIFO.
        assert!(tickets.len() <= 110, "unbounded request/event acceptance");
    }
    let latest = tickets.last().unwrap();
    assert!(
        !latest.cancelled.load(Ordering::Acquire),
        "failed enqueue cancelled usable latest ticket"
    );
    assert!(handle.cancellation.len() <= 110);
    let mut finished = BTreeSet::new();
    let mut cancelled = 0;
    let mut latest_complete = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while finished.len() < tickets.len() && Instant::now() < deadline {
        for event in handle.poll(128) {
            if let ServiceEvent::JobFinished { fact, result } = event {
                assert!(finished.insert(fact.id), "duplicate terminal event");
                match result {
                    Ok(JobOutput::Geometry { binding, .. }) => {
                        if fact.id == latest.id {
                            assert!(binding.matches(&handle.snapshot()));
                            latest_complete = true;
                        }
                    }
                    Err(e) => {
                        assert_eq!(e.code, "job.cancelled");
                        assert!(matches!(fact.state, JobState::Cancelled));
                        cancelled += 1;
                    }
                    _ => panic!(),
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(finished.len(), tickets.len());
    assert!(latest_complete);
    assert!(cancelled > 0);
    assert!(handle.cancellation.is_empty());
    assert!(handle.jobs().len() <= 32);
    // A ticket's cancellation token held by the caller is not retained in the history.
    assert!(handle.job(JobOperation::Geometry).is_ok());
}

#[test]
fn explicitly_cancelled_pending_geometry_never_publishes_or_populates_cache() {
    let mut session = EditorSession::new(scene_data::current_map()).unwrap();
    let mut cache = editor_cook::GeometryCache::default();
    let cancel = AtomicBool::new(true);
    assert_eq!(
        run_job(&mut session, JobOperation::Geometry, &cancel, &mut cache)
            .unwrap_err()
            .code,
        "job.cancelled"
    );
    let mut source = scene_data::current_map();
    source.objects.push(scene_data::ObjectRecord {
        id: scene_data::StableId::from_u128(9000),
        parent: None,
        sibling_order: 9000,
        name: "ground".into(),
        local: Default::default(),
        data: scene_data::ObjectData::Terrain(
            scene_data::terrain::TerrainDocument::flat(
                1,
                [1.0, 1.0],
                vec![(
                    scene_data::StableId::from_u128(9001),
                    scene_data::terrain::TileKey { x: 0, z: 0 },
                )],
            )
            .unwrap(),
        ),
    });
    assert_eq!(
        cache
            .cook(Arc::new(source), &|| Ok(()))
            .unwrap()
            .work
            .rebuilt_tiles,
        1
    );
}
