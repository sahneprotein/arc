//! Landscape rules as the panel handles them: plain data, no drawing.
//!
//! A rule is one entry of the landscape's rule list in the level. The core has two kinds: one
//! scatters grass, one fills a layer from the ground's steepness and height. A rule has no
//! name in the level, so the panel describes it by what it does. What a rule produces is the
//! core's to work out; nothing here computes weights or places grass.
//!
//! The level's rule list is always sent whole. Before it goes, [`tidy`] makes each rule
//! declare what the core wants declared: the layers it reads, and the rules that fill them.

use std::collections::BTreeMap;

use scene_data::{
    terrain::{LayerDefinition, RuleDefinition},
    LayerId, RuleId,
};

/// The core's name of the rule that scatters grass.
pub const GRASS: &str = "grass.scatter";

/// The core's name of the rule that fills a layer from steepness and height.
pub const SLOPE: &str = "layer.slope_height";

/// A number a rule can be given: its key in the level, what the panel calls it, the range the
/// core takes, and the value the core uses when the level does not say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// Whether only whole numbers are taken.
    pub whole: bool,
}

const fn setting(
    key: &'static str,
    label: &'static str,
    min: f32,
    max: f32,
    default: f32,
) -> Setting {
    Setting {
        key,
        label,
        min,
        max,
        default,
        whole: false,
    }
}

/// The settings of a grass rule, in the order the panel shows them.
pub const GRASS_SETTINGS: [Setting; 5] = [
    setting("density", "Density (0 to 1)", 0.0, 1.0, 1.0),
    Setting {
        whole: true,
        ..setting("candidates_per_cell", "Tries per cell", 1.0, 16.0, 4.0)
    },
    setting("height_m", "Height (m)", 0.01, 3.0, 0.4),
    setting("width_m", "Width (m)", 0.01, 1.0, 0.08),
    setting("exclusion_distance_m", "Keeps clear (m)", 0.0, 10.0, 1.0),
];

/// The settings of a slope and height rule, in the order the panel shows them.
pub const SLOPE_SETTINGS: [Setting; 6] = [
    setting("slope_min", "Steepness from", 0.0, 1.0, 0.0),
    setting("slope_max", "Steepness to", 0.0, 1.0, 1.0),
    setting("slope_falloff", "Steepness fade", 0.0, 1.0, 0.1),
    setting(
        "height_min_m",
        "Height from (m)",
        -100_000.0,
        100_000.0,
        -100_000.0,
    ),
    setting(
        "height_max_m",
        "Height to (m)",
        -100_000.0,
        100_000.0,
        100_000.0,
    ),
    setting("height_falloff_m", "Height fade (m)", 0.0, 100_000.0, 1.0),
];

/// The settings of a rule of this kind; none for a kind this editor does not know.
pub fn settings(algorithm: &str) -> &'static [Setting] {
    match algorithm {
        GRASS => &GRASS_SETTINGS,
        SLOPE => &SLOPE_SETTINGS,
        _ => &[],
    }
}

/// The value a rule has for a setting: what the level says, or the core's own value.
pub fn value(rule: &RuleDefinition, setting: &Setting) -> f32 {
    rule.parameters
        .get(setting.key)
        .copied()
        .unwrap_or(setting.default)
}

/// A new grass rule that grows on `density`, or everywhere without one. The core's own
/// settings apply until the person changes one.
pub fn new_grass(id: RuleId, seed: u64, density: Option<LayerId>) -> RuleDefinition {
    RuleDefinition {
        id,
        algorithm: GRASS.to_owned(),
        version: 1,
        seed,
        input_layers: Vec::new(),
        input_rules: Vec::new(),
        output_layer: None,
        density_layer: density,
        exclusion_layers: Vec::new(),
        parameters: BTreeMap::new(),
    }
}

/// Where a new slope rule's steepness band begins. The rule is for what lies on steep ground,
/// rock for instance, so it starts well above flat.
pub const NEW_SLOPE_FROM: f32 = 0.25;

/// A new rule that fills `output` on steep ground.
pub fn new_slope(id: RuleId, seed: u64, output: LayerId) -> RuleDefinition {
    RuleDefinition {
        id,
        algorithm: SLOPE.to_owned(),
        version: 1,
        seed,
        input_layers: Vec::new(),
        input_rules: Vec::new(),
        output_layer: Some(output),
        density_layer: None,
        exclusion_layers: Vec::new(),
        parameters: BTreeMap::from([("slope_min".to_owned(), NEW_SLOPE_FROM)]),
    }
}

/// The rule that fills `layer`, if one does.
pub fn producer<'a>(rules: &'a [RuleDefinition], layer: &LayerId) -> Option<&'a RuleDefinition> {
    rules
        .iter()
        .find(|rule| rule.output_layer.as_ref() == Some(layer))
}

/// Makes every rule declare what the core wants declared, and nothing that is gone.
///
/// A grass rule reads exactly the layer it grows on and the layers that keep it out. Every
/// rule depends on the rules that fill the layers it reads. A grass rule may also name other
/// grass rules to keep clear of; those are kept while they exist. IDs, seeds and settings
/// are never touched.
pub fn tidy(mut rules: Vec<RuleDefinition>) -> Vec<RuleDefinition> {
    let before = rules.clone();
    for rule in &mut rules {
        if rule.algorithm == GRASS {
            let mut reads: Vec<LayerId> = rule
                .density_layer
                .iter()
                .chain(&rule.exclusion_layers)
                .cloned()
                .collect();
            reads.sort();
            reads.dedup();
            rule.input_layers = reads;
        }
        let mut depends: Vec<RuleId> = rule
            .input_layers
            .iter()
            .filter_map(|layer| producer(&before, layer))
            .map(|producer| producer.id.clone())
            .chain(
                rule.input_rules
                    .iter()
                    .filter(|other| {
                        // Grass to keep clear of, as far as it still exists.
                        before
                            .iter()
                            .any(|rule| &rule.id == *other && rule.algorithm == GRASS)
                    })
                    .cloned(),
            )
            .filter(|other| *other != rule.id)
            .collect();
        depends.sort();
        depends.dedup();
        rule.input_rules = depends;
    }
    rules
}

/// The rules as they must be once `layer` is removed from the landscape: a rule that fills it
/// goes with it, and no other rule reads it any more.
pub fn without_layer(rules: &[RuleDefinition], layer: &LayerId) -> Vec<RuleDefinition> {
    let rules = rules
        .iter()
        .filter(|rule| rule.output_layer.as_ref() != Some(layer))
        .cloned()
        .map(|mut rule| {
            if rule.density_layer.as_ref() == Some(layer) {
                rule.density_layer = None;
            }
            rule.exclusion_layers.retain(|other| other != layer);
            rule.input_layers.retain(|other| other != layer);
            rule
        })
        .collect();
    tidy(rules)
}

fn layer_name(layers: &[LayerDefinition], layer: &LayerId) -> String {
    layers
        .iter()
        .find(|definition| &definition.id == layer)
        .map_or_else(
            || "a layer that is gone".to_owned(),
            |definition| definition.name.clone(),
        )
}

/// What a rule does, in the words of its row in the list.
pub fn describe(rule: &RuleDefinition, layers: &[LayerDefinition]) -> String {
    match rule.algorithm.as_str() {
        GRASS => match &rule.density_layer {
            Some(layer) => format!("Grass on {}", layer_name(layers, layer)),
            None => "Grass everywhere".to_owned(),
        },
        SLOPE => match &rule.output_layer {
            Some(layer) => format!("{} by steepness", layer_name(layers, layer)),
            None => "A layer by steepness".to_owned(),
        },
        other => format!("A rule this editor does not know ({other})"),
    }
}

/// What the person should know about flat ground under a slope rule, if anything: the core
/// gives a layer nothing at the very start of its steepness band while the band fades in, so
/// a band that starts at flat leaves perfectly flat ground empty.
pub fn flat_ground_note(rule: &RuleDefinition) -> Option<&'static str> {
    if rule.algorithm != SLOPE {
        return None;
    }
    let from = value(rule, &SLOPE_SETTINGS[0]);
    let fade = value(rule, &SLOPE_SETTINGS[2]);
    if from > 0.0 {
        Some("Flat ground is below this band and gets none of the layer.")
    } else if fade > 0.0 {
        Some(
            "Perfectly flat ground gets none of the layer: the band fades in from nothing at \
             its start. Set the steepness fade to 0 to include flat ground.",
        )
    } else {
        None
    }
}

/// The layers a rule may be given in a place where it reads or fills one, in the order a
/// click steps through them: the next after `now`, around to "none" where that is allowed.
pub fn next_layer(
    choices: &[LayerId],
    now: Option<&LayerId>,
    none_allowed: bool,
) -> Option<LayerId> {
    let at = now.and_then(|now| choices.iter().position(|choice| choice == now));
    match at {
        // From "none", or from a layer that is no longer a choice, to the first.
        None => choices
            .first()
            .cloned()
            .or_else(|| now.filter(|_| !none_allowed).cloned()),
        Some(index) if index + 1 < choices.len() => Some(choices[index + 1].clone()),
        Some(_) if none_allowed => None,
        Some(_) => choices.first().cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene_data::StableId;

    fn id(number: u128) -> StableId {
        StableId::from_u128(number)
    }

    #[test]
    fn a_grass_rule_declares_exactly_the_layers_it_reads() {
        let mut grass = new_grass(id(1), 7, Some(id(10)));
        grass.exclusion_layers = vec![id(12), id(11), id(12)];
        let tidied = tidy(vec![grass]);
        assert_eq!(tidied[0].input_layers, vec![id(10), id(11), id(12)]);
        assert!(tidied[0].input_rules.is_empty());
        // Nothing else moved.
        assert_eq!(
            (tidied[0].seed, tidied[0].density_layer.clone()),
            (7, Some(id(10)))
        );
    }

    #[test]
    fn a_rule_depends_on_the_rule_that_fills_a_layer_it_reads() {
        let rock = new_slope(id(2), 1, id(11));
        let mut grass = new_grass(id(1), 1, Some(id(10)));
        grass.exclusion_layers = vec![id(11)];
        let tidied = tidy(vec![grass, rock]);
        assert_eq!(tidied[0].input_rules, vec![id(2)]);
        assert!(tidied[1].input_rules.is_empty());
    }

    #[test]
    fn a_dependency_on_a_rule_that_is_gone_is_dropped_and_grass_to_keep_clear_of_stays() {
        let mut meadow = new_grass(id(1), 1, None);
        meadow.input_rules = vec![id(3), id(9)];
        let reeds = new_grass(id(3), 1, None);
        let tidied = tidy(vec![meadow, reeds]);
        assert_eq!(tidied[0].input_rules, vec![id(3)]);
    }

    #[test]
    fn removing_a_layer_takes_the_rule_that_fills_it_and_every_reading_of_it() {
        let rock = new_slope(id(2), 1, id(11));
        let mut grass = new_grass(id(1), 1, Some(id(11)));
        grass.exclusion_layers = vec![id(11), id(12)];
        let left = without_layer(&tidy(vec![grass, rock]), &id(11));
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].density_layer, None);
        assert_eq!(left[0].exclusion_layers, vec![id(12)]);
        assert_eq!(left[0].input_layers, vec![id(12)]);
        assert!(left[0].input_rules.is_empty());
    }

    #[test]
    fn a_new_slope_rule_starts_above_flat_and_says_so() {
        let rule = new_slope(id(2), 1, id(11));
        assert_eq!(value(&rule, &SLOPE_SETTINGS[0]), NEW_SLOPE_FROM);
        assert!(flat_ground_note(&rule)
            .unwrap()
            .starts_with("Flat ground is below"));
    }

    #[test]
    fn a_band_from_flat_that_fades_in_warns_and_one_without_fade_does_not() {
        let mut rule = new_slope(id(2), 1, id(11));
        rule.parameters.insert("slope_min".into(), 0.0);
        assert!(flat_ground_note(&rule)
            .unwrap()
            .starts_with("Perfectly flat"));
        rule.parameters.insert("slope_falloff".into(), 0.0);
        assert_eq!(flat_ground_note(&rule), None);
        assert_eq!(flat_ground_note(&new_grass(id(1), 1, None)), None);
    }

    #[test]
    fn a_click_steps_through_the_layers_and_through_none_where_allowed() {
        let layers = [id(10), id(11)];
        assert_eq!(next_layer(&layers, None, true), Some(id(10)));
        assert_eq!(next_layer(&layers, Some(&id(10)), true), Some(id(11)));
        assert_eq!(next_layer(&layers, Some(&id(11)), true), None);
        assert_eq!(next_layer(&layers, Some(&id(11)), false), Some(id(10)));
        // A layer that is no longer a choice gives way to the first that is.
        assert_eq!(next_layer(&layers, Some(&id(99)), false), Some(id(10)));
        // With nothing to choose from, what is there stays.
        assert_eq!(next_layer(&[], Some(&id(99)), false), Some(id(99)));
        assert_eq!(next_layer(&[], None, true), None);
    }

    #[test]
    fn a_rule_is_described_by_what_it_does() {
        let layers = [LayerDefinition {
            id: id(10),
            name: "Meadow".into(),
            kind: scene_data::terrain::LayerKind::Mask,
            color_linear_rgba: None,
        }];
        assert_eq!(
            describe(&new_grass(id(1), 1, Some(id(10))), &layers),
            "Grass on Meadow"
        );
        assert_eq!(
            describe(&new_grass(id(1), 1, None), &layers),
            "Grass everywhere"
        );
        assert_eq!(
            describe(&new_slope(id(2), 1, id(10)), &layers),
            "Meadow by steepness"
        );
    }

    #[test]
    fn every_setting_starts_inside_the_range_the_core_takes() {
        for setting in GRASS_SETTINGS.iter().chain(&SLOPE_SETTINGS) {
            assert!(
                (setting.min..=setting.max).contains(&setting.default),
                "{}",
                setting.key
            );
        }
    }
}
