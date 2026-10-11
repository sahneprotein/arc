use crate::*;
use scene_data::{ObjectData, ObjectRecord};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default)]
pub struct RegionRevisions {
    pub hierarchy: u64,
    pub selection: u64,
    pub inspector: u64,
    pub jobs: u64,
    pub status: u64,
    pub review: u64,
}
#[derive(Clone, Debug)]
pub struct ViewSnapshot {
    pub session: u64,
    pub document_revision: u64,
    pub view_revision: u64,
    pub regions: RegionRevisions,
    pub document: Arc<SceneDocument>,
    pub hierarchy: Arc<[HierarchyRow]>,
    pub selection: Arc<[ObjectId]>,
    pub active: Option<ObjectId>,
    pub pivot: GizmoPivot,
    pub snap: SnapSettings,
    pub review: Arc<[ReviewProblem]>,
    pub changed_objects: Arc<[ObjectId]>,
    pub inspector: Arc<[FieldDescriptor]>,
    pub preview: Option<PreviewSnapshot>,
    pub status: StatusFacts,
    pub jobs: Arc<[JobFact]>,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}
#[derive(Clone, Debug)]
pub struct HierarchyRow {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub name: String,
    pub depth: u32,
    pub child_count: u32,
    pub subtree_end_exclusive: usize,
}
#[derive(Clone, Debug)]
pub struct FieldDescriptor {
    pub id: String,
    pub label_key: String,
    pub unit_key: Option<String>,
    pub kind: FieldKind,
    pub value: FieldValue,
    pub mixed_selection: bool,
    pub read_only: bool,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub step: Option<f32>,
    pub validation: Arc<[Diagnostic]>,
}
#[derive(Clone, Debug)]
pub enum FieldKind {
    Text,
    Number,
    Vector(u8),
    Toggle,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Text(String),
    Number(f32),
    Vector(Vec<f32>),
    Toggle(bool),
}
#[derive(Clone, Debug, Default)]
pub struct StatusFacts {
    pub dirty: bool,
    pub saved_source_hash: Option<ContentHash>,
    pub cook_needed: bool,
    pub simulation_hash: Option<ContentHash>,
    pub open_us: Option<u64>,
    pub cook_us: Option<u64>,
    pub world_ready_us: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct PreviewSnapshot {
    pub transaction: u64,
    pub sequence: u64,
    pub document: Arc<SceneDocument>,
    pub dirty: Arc<[TerrainDirty]>,
}
pub type TerrainDirty = scene_data::terrain::DirtyRegion;

pub(crate) fn hierarchy(scene: &SceneDocument) -> Arc<[HierarchyRow]> {
    use std::collections::BTreeMap;
    let mut children: BTreeMap<Option<&ObjectId>, Vec<&ObjectRecord>> = BTreeMap::new();
    for o in &scene.objects {
        children.entry(o.parent.as_ref()).or_default().push(o);
    }
    for list in children.values_mut() {
        list.sort_by_key(|o| (o.sibling_order, &o.id));
    }
    fn walk(
        children: &BTreeMap<Option<&ObjectId>, Vec<&ObjectRecord>>,
        parent: Option<&ObjectId>,
        depth: u32,
        rows: &mut Vec<HierarchyRow>,
    ) {
        for o in children.get(&parent).into_iter().flatten() {
            let index = rows.len();
            rows.push(HierarchyRow {
                id: o.id.clone(),
                parent: o.parent.clone(),
                name: o.name.clone(),
                depth,
                child_count: children.get(&Some(&o.id)).map_or(0, |c| c.len() as u32),
                subtree_end_exclusive: 0,
            });
            walk(children, Some(&o.id), depth + 1, rows);
            rows[index].subtree_end_exclusive = rows.len();
        }
    }
    let mut rows = Vec::with_capacity(scene.objects.len());
    walk(&children, None, 0, &mut rows);
    rows.into()
}
pub(crate) fn inspector(scene: &SceneDocument, selection: &[ObjectId]) -> Arc<[FieldDescriptor]> {
    let selected: Vec<_> = selection.iter().filter_map(|id| scene.object(id)).collect();
    let Some(first) = selected.first() else {
        return Arc::from([]);
    };
    let mut fields = vec![
        field(
            "name",
            FieldValue::Text(first.name.clone()),
            FieldKind::Text,
            None,
        ),
        field(
            "translation",
            FieldValue::Vector(first.local.translation_m.to_vec()),
            FieldKind::Vector(3),
            Some("metres"),
        ),
        field(
            "rotation",
            FieldValue::Vector(first.local.rotation_xyzw.to_vec()),
            FieldKind::Vector(4),
            None,
        ),
    ];
    if let ObjectData::Box { half_extents_m } = &first.data
        && selected
            .iter()
            .all(|o| matches!(o.data, ObjectData::Box { .. }))
    {
        let mut f = field(
            "half_extents",
            FieldValue::Vector(half_extents_m.to_vec()),
            FieldKind::Vector(3),
            Some("metres"),
        );
        f.min = Some(f32::MIN_POSITIVE);
        fields.push(f);
    }
    for f in &mut fields {
        f.mixed_selection = selected.iter().skip(1).any(|o| match f.id.as_str() {
            "name" => o.name != first.name,
            "translation" => o.local.translation_m != first.local.translation_m,
            "rotation" => o.local.rotation_xyzw != first.local.rotation_xyzw,
            "half_extents" => o.data != first.data,
            _ => false,
        });
    }
    fields.into()
}
fn field(id: &str, value: FieldValue, kind: FieldKind, unit: Option<&str>) -> FieldDescriptor {
    FieldDescriptor {
        id: id.into(),
        label_key: format!("field.{id}"),
        unit_key: unit.map(String::from),
        kind,
        value,
        mixed_selection: false,
        read_only: false,
        min: None,
        max: None,
        step: Some(0.1),
        validation: Arc::from([]),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Geometry,
    Open,
    Save,
    Cook,
    VerifyPackage,
}
#[derive(Clone, Debug)]
pub enum JobState {
    Running,
    Complete,
    Cancelled,
    Failed(Diagnostic),
}
#[derive(Clone, Debug)]
pub struct JobFact {
    pub id: u64,
    pub kind: JobKind,
    pub state: JobState,
    pub elapsed_us: Option<u64>,
}

/// Used to reject viewport job output after document/transaction changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeometryBinding {
    pub session: u64,
    pub revision: u64,
    pub preview: Option<(u64, u64)>,
}
impl GeometryBinding {
    pub fn from_snapshot(view: &ViewSnapshot) -> Self {
        Self {
            session: view.session,
            revision: view.document_revision,
            preview: view.preview.as_ref().map(|p| (p.transaction, p.sequence)),
        }
    }
    pub fn matches(&self, view: &ViewSnapshot) -> bool {
        *self == Self::from_snapshot(view)
    }
}
