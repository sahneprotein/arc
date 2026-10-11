use crate::*;
use scene_data::{ObjectData, SimulationPayload, VisualPayload};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewScope {
    Scene,
    Play,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewProblem {
    pub scope: ReviewScope,
    pub diagnostic: Diagnostic,
}

/// Shared runtime capability facts. Validation of source/payload syntax remains
/// with scene-data/package verification. No cook freshness or executable policy.
#[derive(Clone, Debug, Default)]
pub struct PlayCapabilityFacts {
    pub spawns: usize,
    pub boxes: usize,
    pub terrain: Vec<ProceduralTerrainFacts>,
}
#[derive(Clone, Debug)]
pub struct ProceduralTerrainFacts {
    pub object: ObjectId,
    pub rules: usize,
    pub decoration: usize,
    pub derived_layers: usize,
}

/// Authoritative capability producer used by Review and external Play preflight.
/// Terrain blockers precede spawn/limit blockers, preserving preflight priority.
pub fn play_problems(facts: &PlayCapabilityFacts) -> Vec<ReviewProblem> {
    let mut problems = Vec::new();
    for reason in ["rules", "decoration", "derived_layers"] {
        let mut terrain: Vec<_> = facts.terrain.iter().collect();
        terrain.sort_by_key(|t| &t.object);
        for t in terrain {
            let count = match reason {
                "rules" => t.rules,
                "decoration" => t.decoration,
                _ => t.derived_layers,
            };
            if count > 0 {
                problems.push(ReviewProblem {
                    scope: ReviewScope::Play,
                    diagnostic: Diagnostic::new("play.terrain_unsupported")
                        .with("reason", reason)
                        .with("count", count)
                        .at(&t.object),
                });
            }
        }
    }
    if facts.spawns == 0 {
        problems.push(ReviewProblem {
            scope: ReviewScope::Play,
            diagnostic: Diagnostic::new("play.spawn_required"),
        });
    }
    if facts.boxes > 100_000 || facts.spawns > 100_000 {
        problems.push(ReviewProblem {
            scope: ReviewScope::Play,
            diagnostic: Diagnostic::new("play.runtime_limit"),
        });
    }
    problems
}

pub fn review_play_payloads(
    simulation: &SimulationPayload,
    visual: &VisualPayload,
) -> Vec<ReviewProblem> {
    play_problems(&PlayCapabilityFacts {
        spawns: simulation.spawns.len(),
        boxes: simulation.boxes.len(),
        terrain: visual
            .terrain
            .iter()
            .map(|t| ProceduralTerrainFacts {
                object: t.object.clone(),
                rules: t.rules.len(),
                decoration: t.decoration.len(),
                derived_layers: t.derived_layers.len(),
            })
            .collect(),
    })
}

/// Stable object/code ordering for structural problems. Authored rules alone
/// already block Play; their generated outputs are reported from cooked payloads.
/// No procedural outputs are guessed or evaluated on the snapshot path.
pub fn review_document(scene: &SceneDocument) -> Vec<ReviewProblem> {
    let mut errors = scene_data::validation_diagnostics(scene);
    errors.sort_by(|a, b| {
        (&a.object, &a.code, &a.parameters).cmp(&(&b.object, &b.code, &b.parameters))
    });
    errors.dedup();
    let mut problems: Vec<_> = errors
        .into_iter()
        .map(|diagnostic| ReviewProblem {
            scope: ReviewScope::Scene,
            diagnostic,
        })
        .collect();
    let mut facts = PlayCapabilityFacts::default();
    for object in &scene.objects {
        match &object.data {
            ObjectData::Spawn { .. } => facts.spawns += 1,
            ObjectData::Box { .. } => facts.boxes += 1,
            ObjectData::Terrain(t) => facts.terrain.push(ProceduralTerrainFacts {
                object: object.id.clone(),
                rules: t.rules.len(),
                decoration: 0,
                derived_layers: 0,
            }),
            _ => (),
        }
    }
    problems.extend(play_problems(&facts));
    problems
}
