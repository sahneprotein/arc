use crate::*;
use glam::{Quat, Vec3};
use scene_data::{ObjectData, ObjectRecord, RigidTransform};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapSettings {
    pub enabled: bool,
    pub translation_m: f32,
    pub rotation_degrees: f32,
}
impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            translation_m: 0.5,
            rotation_degrees: 15.0,
        }
    }
}
impl SnapSettings {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        for (field, value, min, max) in [
            ("translation_m", self.translation_m, 0.01, 100.0),
            ("rotation_degrees", self.rotation_degrees, 1.0, 90.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(Diagnostic::new("snap.range")
                    .with("field", field)
                    .with("min", min)
                    .with("max", max));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoPivot {
    Active,
    Center,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformRetention {
    KeepWorld,
    KeepLocal,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldAssignment {
    Name(String),
    /// Local metres; None preserves each object's own axis value.
    Translation([Option<f32>; 3]),
    /// Full box size in metres, converted to stored half-extents.
    Size([Option<f32>; 3]),
}

/// UI supplies fresh identities and product placement/name policy. Every
/// descendant must have a spec. Parents are derived/remapped by the core.
#[derive(Clone, Debug, PartialEq)]
pub struct DuplicateObject {
    pub source: ObjectId,
    pub id: ObjectId,
    pub name: String,
    pub sibling_order: u32,
    /// Required for spawns and forbidden for other kinds; must be unused.
    pub spawn_order: Option<u32>,
    /// Roots may be offset by the caller; descendant locals must be preserved.
    pub local: RigidTransform,
}

pub(crate) fn assign_field(
    scene: &SceneDocument,
    objects: &[ObjectId],
    assignment: &FieldAssignment,
) -> Result<Vec<Mutation>, Diagnostic> {
    objects
        .iter()
        .map(|id| {
            let object = scene
                .object(id)
                .ok_or_else(|| Diagnostic::new("object.missing").at(id))?;
            match assignment {
                FieldAssignment::Name(name) => Ok(Mutation::Rename {
                    object: id.clone(),
                    name: name.clone(),
                }),
                FieldAssignment::Translation(values) => {
                    let mut value = object.local.clone();
                    for (axis, assigned) in values.iter().enumerate() {
                        if let Some(assigned) = assigned {
                            value.translation_m[axis] = *assigned;
                        }
                    }
                    Ok(Mutation::Transform {
                        object: id.clone(),
                        value,
                    })
                }
                FieldAssignment::Size(values) => {
                    let ObjectData::Box { half_extents_m } = object.data else {
                        return Err(Diagnostic::new("field.kind").at(id));
                    };
                    let mut value = half_extents_m;
                    for (axis, assigned) in values.iter().enumerate() {
                        if let Some(assigned) = assigned {
                            if !assigned.is_finite() || *assigned <= 0.0 {
                                return Err(Diagnostic::new("box.extents").at(id));
                            }
                            value[axis] = *assigned / 2.0;
                        }
                    }
                    Ok(Mutation::BoxExtents {
                        object: id.clone(),
                        value,
                    })
                }
            }
        })
        .collect()
}

/// Collapse descendants of another requested object while preserving input order.
pub(crate) fn roots(
    scene: &SceneDocument,
    objects: &[ObjectId],
) -> Result<Vec<ObjectId>, Diagnostic> {
    let selected: BTreeSet<_> = objects.iter().collect();
    let mut result = Vec::new();
    for id in objects {
        let object = scene
            .object(id)
            .ok_or_else(|| Diagnostic::new("object.missing").at(id))?;
        let mut parent = object.parent.as_ref();
        let mut nested = false;
        while let Some(id) = parent {
            if selected.contains(id) {
                nested = true;
                break;
            }
            parent = scene.object(id).and_then(|o| o.parent.as_ref());
        }
        if !nested && !result.contains(id) {
            result.push(id.clone());
        }
    }
    Ok(result)
}

pub(crate) struct Duplication {
    pub mutations: Vec<Mutation>,
    pub selected: Vec<ObjectId>,
    pub remap: BTreeMap<ObjectId, ObjectId>,
}
pub(crate) fn duplicate_objects(
    scene: &SceneDocument,
    selection: &[ObjectId],
    copies: &[DuplicateObject],
) -> Result<Duplication, Diagnostic> {
    if selection.is_empty() || copies.is_empty() {
        return Err(Diagnostic::new("selection.empty"));
    }
    let roots = roots(scene, selection)?;
    let mut required: BTreeSet<_> = roots.iter().cloned().collect();
    // Each hierarchy is bounded by validation; traverse once instead of a pass per depth.
    let mut children: BTreeMap<&ObjectId, Vec<&ObjectId>> = BTreeMap::new();
    for object in &scene.objects {
        if let Some(parent) = &object.parent {
            children.entry(parent).or_default().push(&object.id);
        }
    }
    let mut pending = roots.clone();
    while let Some(id) = pending.pop() {
        for child in children.get(&id).into_iter().flatten() {
            if required.insert((*child).clone()) {
                pending.push((*child).clone());
            }
        }
    }
    let mut remap = BTreeMap::new();
    let mut new_ids = BTreeSet::new();
    for copy in copies {
        if !required.contains(&copy.source)
            || remap.insert(copy.source.clone(), copy.id.clone()).is_some()
        {
            return Err(Diagnostic::new("duplicate.sources").at(&copy.source));
        }
        if !copy.id.valid() || scene.object(&copy.id).is_some() || !new_ids.insert(copy.id.clone())
        {
            return Err(Diagnostic::new("id.invalid_or_duplicate").at(&copy.id));
        }
    }
    if remap.len() != required.len() {
        return Err(Diagnostic::new("duplicate.incomplete"));
    }
    let mut last_orders = BTreeMap::new();
    for object in &scene.objects {
        last_orders
            .entry(object.parent.clone())
            .and_modify(|order: &mut u32| *order = (*order).max(object.sibling_order))
            .or_insert(object.sibling_order);
    }
    let mut mutations = Vec::new();
    for copy in copies {
        let source = scene.object(&copy.source).unwrap();
        if matches!(source.data, ObjectData::Terrain(_)) {
            return Err(Diagnostic::new("duplicate.terrain_unsupported").at(&source.id));
        }
        let parent = source
            .parent
            .as_ref()
            .map(|id| remap.get(id).unwrap_or(id).clone());
        if roots.contains(&source.id) {
            if last_orders
                .get(&parent)
                .is_some_and(|order| copy.sibling_order <= *order)
            {
                return Err(Diagnostic::new("duplicate.append_order").at(&source.id));
            }
            last_orders.insert(parent.clone(), copy.sibling_order);
        } else if copy.local != source.local || copy.sibling_order != source.sibling_order {
            return Err(Diagnostic::new("duplicate.child_changed").at(&source.id));
        }
        let data = match (&source.data, copy.spawn_order) {
            (ObjectData::Spawn { .. }, Some(order)) => ObjectData::Spawn { order },
            (ObjectData::Spawn { .. }, None) | (_, Some(_)) => {
                return Err(Diagnostic::new("duplicate.spawn_order").at(&source.id))
            }
            _ => source.data.clone(),
        };
        mutations.push(Mutation::Create(ObjectRecord {
            id: copy.id.clone(),
            parent,
            sibling_order: copy.sibling_order,
            name: copy.name.clone(),
            local: copy.local.clone(),
            data,
        }));
    }
    let selected = roots.iter().map(|id| remap[id].clone()).collect();
    Ok(Duplication {
        mutations,
        selected,
        remap,
    })
}

pub(crate) fn reparent_objects(
    scene: &SceneDocument,
    objects: &[ObjectId],
    parent: &Option<ObjectId>,
    keep: TransformRetention,
) -> Result<Vec<Mutation>, Diagnostic> {
    if objects.is_empty() {
        return Err(Diagnostic::new("selection.empty"));
    }
    if let Some(parent) = parent
        && scene.object(parent).is_none()
    {
        return Err(Diagnostic::new("reference.missing_parent").at(parent));
    }
    let objects = roots(scene, objects)?;
    let worlds = scene_data::world_transforms(scene)?;
    let next = scene
        .objects
        .iter()
        .filter(|o| &o.parent == parent)
        .map(|o| o.sibling_order)
        .max();
    let mut order = match next {
        Some(max) => max
            .checked_add(1)
            .ok_or_else(|| Diagnostic::new("hierarchy.order_limit"))?,
        None => 0,
    };
    let mut mutations = Vec::new();
    for (index, id) in objects.iter().enumerate() {
        if index > 0 {
            order = order
                .checked_add(1)
                .ok_or_else(|| Diagnostic::new("hierarchy.order_limit").at(id))?;
        }
        mutations.push(Mutation::Reparent {
            object: id.clone(),
            parent: parent.clone(),
            order,
        });
        if keep == TransformRetention::KeepWorld {
            let world = &worlds[id];
            let value = if let Some(parent) = parent {
                let target = &worlds[parent];
                let inverse = Quat::from_array(target.rotation_xyzw).inverse();
                RigidTransform {
                    translation_m: (inverse
                        * (Vec3::from_array(world.translation_m)
                            - Vec3::from_array(target.translation_m)))
                    .to_array(),
                    rotation_xyzw: (inverse * Quat::from_array(world.rotation_xyzw))
                        .normalize()
                        .to_array(),
                }
            } else {
                world.clone()
            };
            mutations.push(Mutation::Transform {
                object: id.clone(),
                value,
            });
        }
    }
    Ok(mutations)
}
