//! Registered visual rule semantics, explicit dependencies and deterministic ordering.
use crate::{terrain::*, *};
use std::collections::{BTreeMap, BTreeSet};
pub const GRASS_SCATTER: &str = "grass.scatter";
pub const GRASS_VERSION: u32 = 1;
pub const AUTO_LAYER_SLOPE_HEIGHT: &str = "layer.slope_height";
pub const AUTO_LAYER_VERSION: u32 = 1;
pub const MAX_DECORATION: usize = 100_000;
pub const MAX_DERIVED_LAYER_WEIGHTS: u64 = 2_000_000;
pub fn setting(rule: &RuleDefinition, name: &str, default: f32) -> f32 {
    rule.parameters.get(name).copied().unwrap_or(default)
}
pub fn validate_rules<'a>(
    rules: &'a [RuleDefinition],
    layers: &[LayerDefinition],
    cells: u64,
) -> Result<Vec<&'a RuleDefinition>, Diagnostic> {
    if rules.len() > 64 {
        return Err(Diagnostic::new("rule.limit"));
    }
    let layer_ids: BTreeSet<_> = layers.iter().map(|l| &l.id).collect();
    let layer_defs: BTreeMap<_, _> = layers.iter().map(|l| (&l.id, l)).collect();
    let mut registry = BTreeMap::new();
    let mut outputs = BTreeSet::new();
    let mut candidates = 0u64;
    for rule in rules {
        if !rule.id.valid() || registry.insert(&rule.id, rule).is_some() {
            return Err(Diagnostic::new("rule.id"));
        }
        let grass = rule.algorithm == GRASS_SCATTER && rule.version == GRASS_VERSION;
        let auto_layer =
            rule.algorithm == AUTO_LAYER_SLOPE_HEIGHT && rule.version == AUTO_LAYER_VERSION;
        if !grass && !auto_layer {
            return Err(Diagnostic::new("rule.unsupported"));
        }
        if grass && rule.output_layer.is_some() {
            return Err(Diagnostic::new("rule.output_layer"));
        }
        if auto_layer {
            let output = rule
                .output_layer
                .as_ref()
                .and_then(|id| layer_defs.get(id))
                .ok_or_else(|| Diagnostic::new("rule.output_layer"))?;
            if !outputs.insert(output.id.clone())
                || rule.density_layer.is_some()
                || !rule.exclusion_layers.is_empty()
            {
                return Err(Diagnostic::new("rule.output_layer"));
            }
            if rule
                .input_layers
                .iter()
                .any(|id| !matches!(layer_defs.get(id).map(|l| &l.kind), Some(LayerKind::Mask)))
            {
                return Err(Diagnostic::new("rule.layer_binding"));
            }
        }
        for (name, value) in &rule.parameters {
            let valid = match (rule.algorithm.as_str(), name.as_str()) {
                (GRASS_SCATTER, "candidates_per_cell") => {
                    *value >= 1.0 && *value <= 16.0 && *value == (*value as u32) as f32
                }
                (GRASS_SCATTER, "density") => (0.0..=1.0).contains(value),
                (GRASS_SCATTER, "height_m") => *value > 0.0 && *value <= 3.0,
                (GRASS_SCATTER, "width_m") => *value > 0.0 && *value <= 1.0,
                (GRASS_SCATTER, "exclusion_distance_m") => (0.0..=10.0).contains(value),
                (AUTO_LAYER_SLOPE_HEIGHT, "slope_min") => (0.0..=1.0).contains(value),
                (AUTO_LAYER_SLOPE_HEIGHT, "slope_max") => (0.0..=1.0).contains(value),
                (AUTO_LAYER_SLOPE_HEIGHT, "slope_falloff") => (0.0..=1.0).contains(value),
                (AUTO_LAYER_SLOPE_HEIGHT, "height_min_m" | "height_max_m") => {
                    (-100_000.0..=100_000.0).contains(value)
                }
                (AUTO_LAYER_SLOPE_HEIGHT, "height_falloff_m") => (0.0..=100_000.0).contains(value),
                _ => false,
            };
            if !value.is_finite() || !valid {
                return Err(Diagnostic::new("rule.parameter").with("field", name));
            }
        }
        if auto_layer
            && (setting(rule, "slope_min", 0.0) >= setting(rule, "slope_max", 1.0)
                || setting(rule, "height_min_m", -100_000.0)
                    >= setting(rule, "height_max_m", 100_000.0))
        {
            return Err(Diagnostic::new("rule.parameter_range"));
        }
        let mut inputs = BTreeSet::new();
        if auto_layer {
            for id in &rule.input_layers {
                if !inputs.insert(id) {
                    return Err(Diagnostic::new("rule.duplicate_binding"));
                }
            }
        } else {
            if let Some(id) = &rule.density_layer {
                inputs.insert(id);
            }
            for id in &rule.exclusion_layers {
                if !inputs.insert(id) {
                    return Err(Diagnostic::new("rule.duplicate_binding"));
                }
            }
        }
        let declared: BTreeSet<_> = rule.input_layers.iter().collect();
        if declared.len() != rule.input_layers.len()
            || declared != inputs
            || inputs.iter().any(|id| !layer_ids.contains(id))
        {
            return Err(Diagnostic::new("rule.layer_binding"));
        }
        let dependencies: BTreeSet<_> = rule.input_rules.iter().collect();
        if dependencies.len() != rule.input_rules.len() {
            return Err(Diagnostic::new("rule.duplicate_dependency"));
        }
        if grass {
            candidates = candidates.saturating_add(cells.saturating_mul(setting(
                rule,
                "candidates_per_cell",
                4.0,
            ) as u64));
        }
        if candidates > 1_000_000 {
            return Err(Diagnostic::new("rule.candidate_limit"));
        }
    }
    fn visit<'a>(
        id: &StableId,
        registry: &BTreeMap<&StableId, &'a RuleDefinition>,
        active: &mut BTreeSet<StableId>,
        done: &mut BTreeSet<StableId>,
        ordered: &mut Vec<&'a RuleDefinition>,
    ) -> Result<(), Diagnostic> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.clone()) {
            return Err(Diagnostic::new("rule.cycle"));
        }
        let rule = *registry
            .get(id)
            .ok_or_else(|| Diagnostic::new("rule.dependency_missing"))?;
        let mut dependencies = rule.input_rules.iter().collect::<Vec<_>>();
        dependencies.sort();
        for dependency in dependencies {
            visit(dependency, registry, active, done, ordered)?;
        }
        active.remove(id);
        done.insert(id.clone());
        ordered.push(rule);
        Ok(())
    }
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    let mut ordered = Vec::new();
    for id in registry.keys() {
        visit(id, &registry, &mut active, &mut done, &mut ordered)?;
    }
    let by_output: BTreeMap<_, _> = rules
        .iter()
        .filter_map(|rule| rule.output_layer.as_ref().map(|layer| (layer, rule)))
        .collect();
    for rule in rules {
        let mut pending = rule.input_rules.to_vec();
        let mut visited = BTreeSet::new();
        while let Some(dependency) = pending.pop() {
            if !visited.insert(dependency.clone()) {
                continue;
            }
            let producer = registry
                .get(&dependency)
                .ok_or_else(|| Diagnostic::new("rule.dependency_missing"))?;
            pending.extend(producer.input_rules.iter().cloned());
        }
        for layer in &rule.input_layers {
            if let Some(producer) = by_output.get(layer)
                && !visited.contains(&producer.id)
            {
                return Err(Diagnostic::new("rule.dependency_missing"));
            }
        }
    }
    Ok(ordered)
}
