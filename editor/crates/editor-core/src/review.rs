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

/// What is known of one particle emitter asset of the game, by whoever reads the game's
/// folder: the core reads no files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitterAsset {
    /// The asset's stable ID, which placements name it by.
    pub id: String,
    pub state: EmitterAssetState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitterAssetState {
    /// The file can be read and the game's particle runtime draws it.
    Usable,
    /// The file cannot be read as an emitter: the reader's code.
    Refused { code: String },
    /// The game's particle runtime does not draw an emitter with this field as it is.
    Unsupported { field: String },
}
/// The game's emitter assets as the session is told them ([`Command::SetEmitterAssets`]).
/// Before it is told anything there is no game and no asset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmitterAssets {
    /// Whether there is a game folder to read emitters from at all.
    pub game: bool,
    pub assets: Vec<EmitterAsset>,
}

/// What stops the game from showing the level's placed emitters, one problem for each
/// placement that has one, by placement ID. A placement whose asset is missing, cannot be
/// read, or cannot be drawn by the game's runtime is never skipped: the level is not
/// cooked or played until it is put right.
pub fn emitter_problems(scene: &SceneDocument, known: &EmitterAssets) -> Vec<ReviewProblem> {
    let mut placements: Vec<_> = scene
        .objects
        .iter()
        .filter_map(|object| match &object.data {
            ObjectData::Emitter { emitter_id } => Some((&object.id, emitter_id)),
            _ => None,
        })
        .collect();
    placements.sort();
    placements
        .into_iter()
        .filter_map(|(object, emitter_id)| {
            let asset = known.assets.iter().find(|asset| &asset.id == emitter_id);
            let diagnostic = match asset.map(|asset| &asset.state) {
                _ if !known.game => Diagnostic::new("emitter.no_game"),
                None => Diagnostic::new("emitter.asset_missing"),
                Some(EmitterAssetState::Usable) => return None,
                Some(EmitterAssetState::Refused { code }) => {
                    Diagnostic::new("emitter.asset_refused").with("code", code)
                }
                Some(EmitterAssetState::Unsupported { field }) => {
                    Diagnostic::new("emitter.runtime_unsupported").with("field", field)
                }
            };
            Some(ReviewProblem {
                scope: ReviewScope::Play,
                diagnostic: diagnostic.with("emitter_id", emitter_id).at(object),
            })
        })
        .collect()
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
    review_document_with(scene, &EmitterAssets::default())
}

/// [`review_document`] with what is known of the game's emitter assets: after everything
/// else, what stops the game from showing the level's placed emitters.
pub fn review_document_with(scene: &SceneDocument, emitters: &EmitterAssets) -> Vec<ReviewProblem> {
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
    problems.extend(emitter_problems(scene, emitters));
    problems
}
