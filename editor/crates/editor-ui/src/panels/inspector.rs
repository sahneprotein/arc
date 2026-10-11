//! The inspector panel: the fields of the selected object, or of several.
//!
//! The core describes each field (kind, label key, unit, value); this panel draws them. An
//! edit is sent to the core as a command when the field is committed, and the shown value
//! follows the core's next snapshot.
//!
//! With several objects selected the panel says how many, and the core says which fields
//! differ between them. Of such a field the numbers the objects share are still fields, and
//! each number that differs is a dash, as the design's PropertyRow has it. The dash stands in
//! a field too: a number typed there, or into any field, goes to every selected object, as
//! one undo step, and the objects then share it. The name is shown and cannot be changed for
//! several at once.
//!
//! The panel also says where the gizmo of several objects is: on one of them, which a
//! Shift-click on another selected object moves, or at their middle. Two buttons choose.
//!
//! At the foot of the panel are the two steps of snapping, the grid in metres and the turn in
//! degrees: settings of the object tools, which this panel is the place of. The core keeps
//! them and refuses a step outside what it takes.
//!
//! Position and half size are number fields. Rotation is three number fields in degrees
//! (turns about X, then Y, then Z); the document keeps a quaternion. The name is shown and
//! not yet editable here.

use bevy::{
    app::PropagateOver,
    feathers::{
        constants::fonts,
        containers::{pane, pane_body, pane_header},
        controls::{
            ButtonVariant, FeathersButton, FeathersNumberInput, FeathersTextInput,
            FeathersTextInputContainer, NumberInputValue, UpdateNumberInput,
        },
        display::{label, label_dim},
        theme::{ThemeBorderColor, ThemeTextColor, ThemeToken, ThemedText},
        tokens,
    },
    input_focus::InputFocus,
    prelude::*,
    text::{EditableText, FontSourceTemplate, TextEdit},
    ui_widgets::{Activate, ValueChange},
};
use editor_core::{
    Command, FieldAssignment, FieldDescriptor, FieldKind, FieldValue, GizmoPivot, Mutation,
    SnapSettings, ViewSnapshot,
};
use scene_data::{ObjectData, ObjectRecord, RigidTransform};

use crate::{
    session::Editor,
    theme::{feathers::BORDER, size, spacing, text},
    wording,
};

/// The inspector panel.
#[derive(Component, Default, Clone)]
pub struct InspectorPanel;

/// The node the fields are rebuilt in.
#[derive(Component, Default, Clone)]
pub(crate) struct InspectorBody;

/// The longest name the name field takes.
const NAME_LENGTH: usize = 64;

/// The text field of the selected object's name, and the name it was last given to show.
#[derive(Component, Default, Clone)]
pub struct NameInput(pub String);

/// One number input of a field: which field, which component of it, and the value it shows.
#[derive(Component, Default, Clone)]
pub struct FieldInput {
    pub field: String,
    pub axis: usize,
    pub value: f32,
    /// Whether the selected objects differ in this number. The input then shows a dash and no
    /// value, and whatever is typed into it is new.
    pub mixed: bool,
}

/// The dash over a number input whose objects differ in that number.
#[derive(Component, Default, Clone)]
pub struct MixedMark;

const AXES: [(&str, ThemeToken); 3] = [
    ("X", tokens::TEXT_INPUT_X_AXIS),
    ("Y", tokens::TEXT_INPUT_Y_AXIS),
    ("Z", tokens::TEXT_INPUT_Z_AXIS),
];

pub fn inspector_panel() -> impl Scene {
    bsn! {
        pane()
        InspectorPanel
        Node { width: px(size::INSPECTOR_WIDTH), flex_shrink: 0.0 }
        Children [
            (pane_header() Children [ (Text("Inspector") ThemedText) ]),
            (
                pane_body()
                InspectorBody
                Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::clip() }
            ),
            (
                pane_body()
                Node { flex_shrink: 0.0, border: UiRect::top(px(1)) }
                ThemeBorderColor(BORDER)
                Children [
                    group_title("Snap"),
                    row(bsn_list![field_label("Grid (m)"), snap_step(SNAP_GRID)]),
                    row(bsn_list![field_label("Turn (\u{b0})"), snap_step(SNAP_TURN)]),
                ]
            ),
        ]
    }
}

/// The title of a group of rows: the design's small text, in capitals.
fn group_title(title: &str) -> impl Scene {
    let title = title.to_uppercase();
    bsn! {
        Text({title})
        TextFont {
            font: FontSourceTemplate::Handle(fonts::REGULAR),
            font_size: {FontSize::Px(text::SMALL.size)},
        }
        PropagateOver<TextFont>
        ThemeTextColor(tokens::TEXT_DIM)
    }
}

/// The name the two snap steps go by as a field: its first number is the grid's step in
/// metres, its second the step of a turn in degrees.
pub const SNAP_FIELD: &str = "snap";
pub const SNAP_GRID: usize = 0;
pub const SNAP_TURN: usize = 1;

/// One of the two snap steps as a number field.
fn snap_step(axis: usize) -> impl Scene {
    let field = SNAP_FIELD.to_owned();
    let value = snap_step_of(SnapSettings::default(), axis);
    bsn! {
        @FeathersNumberInput
        FieldInput { field: {field}, axis: {axis}, value: {value} }
        Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0) }
        on(commit_snap_step)
    }
}

/// The step a snap field shows.
fn snap_step_of(snap: SnapSettings, axis: usize) -> f32 {
    if axis == SNAP_GRID {
        snap.translation_m
    } else {
        snap.rotation_degrees
    }
}

/// The snap setting with the step of one field replaced by what was typed there.
pub fn with_snap_step(mut snap: SnapSettings, axis: usize, value: f32) -> SnapSettings {
    if axis == SNAP_GRID {
        snap.translation_m = value;
    } else {
        snap.rotation_degrees = value;
    }
    snap
}

/// Asks the core for a typed snap step. A step it refuses is reported like any problem, and
/// the setting stays.
fn commit_snap_step(
    change: On<ValueChange<f32>>,
    inputs: Query<&FieldInput>,
    mut editor: NonSendMut<Editor>,
) {
    if !change.is_final {
        return;
    }
    let Ok(input) = inputs.get(change.source) else {
        return;
    };
    if input.value == change.value {
        return;
    }
    let snap = with_snap_step(editor.snapshot().snap, input.axis, change.value);
    editor.set_snap(snap);
}

/// Shows the snap steps the core has, when they changed.
pub(crate) fn fill_snap_steps(
    mut commands: Commands,
    editor: NonSend<Editor>,
    mut inputs: Query<(Entity, &mut FieldInput)>,
) {
    let snap = editor.snapshot().snap;
    for (entity, mut input) in &mut inputs {
        if input.field != SNAP_FIELD {
            continue;
        }
        let value = snap_step_of(snap, input.axis);
        if input.value != value {
            input.value = value;
            commands.trigger(UpdateNumberInput {
                entity,
                value: NumberInputValue::F32(value),
            });
        }
    }
}

fn field_label(text: &str) -> impl Scene {
    bsn! {
        Node { width: px(96), flex_shrink: 0.0 }
        Children [ label(text.to_owned()) ]
    }
}

fn row(children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            flex_shrink: 0.0,
        }
        Children [ {children} ]
    }
}

/// A value as a number field shows it: three decimals, which is a millimetre for lengths.
/// The document keeps the exact value; only what is typed replaces it.
pub fn shown_number(value: f32) -> f32 {
    // Adding zero turns a negative zero into zero: nobody wants to read "-0".
    (value * 1000.0).round() / 1000.0 + 0.0
}

fn number_input(field: &str, axis: usize, value: f32) -> impl Scene {
    let value = shown_number(value);
    let (letter, color) = AXES[axis % AXES.len()].clone();
    let field = field.to_owned();
    bsn! {
        @FeathersNumberInput { @sigil_color: {color}, @label_text: {Some(letter)} }
        FieldInput { field: {field}, axis: {axis}, value: {value} }
        Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0) }
        on(commit_field)
    }
}

/// What a number field shows when the selected objects differ in that number.
const MIXED: &str = "\u{2014}";

/// A number field for a number the selected objects differ in: empty, with the dash over it
/// until it is typed in. The dash takes no click; what is under it is the field.
fn mixed_number(field: &str, axis: usize) -> impl Scene {
    let (letter, color) = AXES[axis % AXES.len()].clone();
    let field = field.to_owned();
    bsn! {
        @FeathersNumberInput { @sigil_color: {color}, @label_text: {Some(letter)} }
        FieldInput { field: {field}, axis: {axis}, mixed: true }
        Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0) }
        on(commit_field)
        Children [(
            MixedMark
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            }
            Pickable { should_block_lower: false, is_hoverable: false }
            Children [(
                label_dim(MIXED)
                Pickable { should_block_lower: false, is_hoverable: false }
            )]
        )]
    }
}

/// Shows the dash of a number the objects differ in while its field is empty and not being
/// typed in, and hides it otherwise.
pub(crate) fn show_mixed_marks(
    focus: Res<InputFocus>,
    inputs: Query<(&FieldInput, &Children)>,
    texts: Query<&EditableText>,
    mut marks: Query<&mut Node, With<MixedMark>>,
) {
    for (input, children) in &inputs {
        if !input.mixed {
            continue;
        }
        let in_use = children.iter().any(|child| {
            texts.get(child).is_ok_and(|text| {
                focus.get() == Some(child) || !text.value().to_string().is_empty()
            })
        });
        let wanted = if in_use { Display::None } else { Display::Flex };
        for child in children.iter() {
            if let Ok(mut node) = marks.get_mut(child)
                && node.display != wanted
            {
                node.display = wanted;
            }
        }
    }
}

/// The three numbers of a field: a number field each, with a dash where the objects differ.
fn numbers(field: &str, values: &[f32], mixed: [bool; 3]) -> Vec<Box<dyn SceneList>> {
    values
        .iter()
        .enumerate()
        .map(|(axis, value)| -> Box<dyn SceneList> {
            if mixed[axis % 3] {
                Box::new(bsn_list![mixed_number(field, axis)])
            } else {
                Box::new(bsn_list![number_input(field, axis, *value)])
            }
        })
        .collect()
}

/// The three numbers of `field` as the panel shows them for one object: a position or a half
/// size in metres, a rotation in degrees. `None` for an object that has no such field.
fn numbers_of(object: &ObjectRecord, field: &str) -> Option<[f32; 3]> {
    match (field, &object.data) {
        ("translation", _) => Some(object.local.translation_m),
        ("rotation", _) => Some(euler_degrees(&object.local.rotation_xyzw)),
        ("half_extents", ObjectData::Box { half_extents_m }) => Some(*half_extents_m),
        _ => None,
    }
}

/// Which of a field's three numbers differ between the selected objects. The core says
/// whether the field differs at all; which of its numbers do is read off the same objects, to
/// the precision the panel shows them with.
pub fn mixed_axes(snapshot: &ViewSnapshot, descriptor: &FieldDescriptor) -> [bool; 3] {
    if !descriptor.mixed_selection {
        return [false; 3];
    }
    let mut selected = snapshot
        .selection
        .iter()
        .filter_map(|id| snapshot.document.object(id));
    let Some(first) = selected
        .next()
        .and_then(|object| numbers_of(object, &descriptor.id))
    else {
        return [true; 3];
    };
    let mut mixed = [false; 3];
    for object in selected {
        match numbers_of(object, &descriptor.id) {
            Some(numbers) => {
                for axis in 0..3 {
                    mixed[axis] |= shown_number(numbers[axis]) != shown_number(first[axis]);
                }
            }
            // An object without the field, such as a spawn point beside a box: every number
            // differs.
            None => return [true; 3],
        }
    }
    mixed
}

/// What the panel says first when several objects are selected.
pub fn several(count: usize) -> Option<String> {
    (count > 1).then(|| format!("{count} objects"))
}

/// The line under it.
const SEVERAL_HINT: &str =
    "A change here goes to all of them. A dash is a value they differ in; type over it to give \
     them one.";

/// Where the gizmo of several selected objects is, in the panel's words. `on` is the name of
/// the object the gizmo is on; `None` when it can hold none of them, as with landscapes only.
pub fn gizmo_line(pivot: GizmoPivot, on: Option<&str>, at_middle: bool) -> String {
    match (pivot, on) {
        (GizmoPivot::Center, _) if at_middle => {
            "The gizmo is at the middle of the selected objects and turns them about it.".into()
        }
        (_, Some(name)) => {
            format!("The gizmo is on {name}. Shift-click another selected object to put it there.")
        }
        (_, None) => "None of the selected objects can hold the gizmo.".into(),
    }
}

/// The selected objects the gizmo moves by themselves: boxes and spawn points that are not
/// under another selected object. A landscape is not moved by the gizmo, and an object under
/// a selected one goes where that one goes.
pub fn gizmo_roots(snapshot: &ViewSnapshot) -> Vec<&ObjectRecord> {
    let document = &snapshot.document;
    let under_a_selected = |object: &ObjectRecord| {
        let mut above = object.parent.as_ref();
        while let Some(parent) = above {
            if snapshot.selection.contains(parent) {
                return true;
            }
            above = document
                .object(parent)
                .and_then(|parent| parent.parent.as_ref());
        }
        false
    };
    snapshot
        .selection
        .iter()
        .filter_map(|id| document.object(id))
        .filter(|object| {
            matches!(
                object.data,
                ObjectData::Box { .. } | ObjectData::Spawn { .. }
            ) && !under_a_selected(object)
        })
        .collect()
}

/// The name of the object the gizmo is on: the one the core calls active if it can hold the
/// gizmo, else the first selected one that can.
pub fn gizmo_object(snapshot: &ViewSnapshot) -> Option<&ObjectRecord> {
    let holds = |object: &&ObjectRecord| {
        matches!(
            object.data,
            ObjectData::Box { .. } | ObjectData::Spawn { .. }
        )
    };
    let document = &snapshot.document;
    snapshot
        .active
        .as_ref()
        .and_then(|id| document.object(id))
        .filter(holds)
        .or_else(|| {
            snapshot
                .selection
                .iter()
                .filter_map(|id| document.object(id))
                .find(holds)
        })
}

/// One of the two buttons that choose where the gizmo of several objects is: on one of them,
/// or (`middle`) at the middle of them all.
#[derive(Component, Default, Clone)]
pub struct PivotSwitch {
    pub middle: bool,
}

fn pivot_button(middle: bool, chosen: bool) -> impl Scene {
    let caption = if middle { "Their middle" } else { "One object" };
    let variant = if chosen {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    bsn! {
        @FeathersButton {
            @caption: bsn! { Text(caption) ThemedText },
            @variant: {variant},
        }
        PivotSwitch { middle: {middle} }
        on(|activate: On<Activate>,
            switches: Query<&PivotSwitch>,
            mut editor: NonSendMut<Editor>| {
            if let Ok(switch) = switches.get(activate.entity) {
                editor.set_pivot(if switch.middle {
                    GizmoPivot::Center
                } else {
                    GizmoPivot::Active
                });
            }
        })
    }
}

/// What the panel says of the gizmo when several objects are selected: the two buttons and
/// the line under them.
fn gizmo_rows(snapshot: &ViewSnapshot) -> Box<dyn SceneList> {
    let middle = snapshot.pivot == GizmoPivot::Center;
    let line = gizmo_line(
        snapshot.pivot,
        gizmo_object(snapshot).map(|object| object.name.as_str()),
        gizmo_roots(snapshot).len() > 1,
    );
    Box::new(bsn_list![
        row(bsn_list![
            field_label("Gizmo at"),
            pivot_button(false, !middle),
            pivot_button(true, middle)
        ]),
        label_dim(line)
    ])
}

/// The scenes of one field: a label and its editor, or its value as text. `mixed` says which
/// of its numbers differ between the selected objects, and `alone` whether one object is
/// selected and not several.
fn field(descriptor: &FieldDescriptor, mixed: [bool; 3], alone: bool) -> Box<dyn SceneList> {
    let name = wording::field_label(&descriptor.label_key).to_owned();
    let unit = descriptor
        .unit_key
        .as_deref()
        .map(wording::unit)
        .unwrap_or_default();
    let title = if unit.is_empty() {
        name.clone()
    } else {
        format!("{name} ({unit})")
    };
    match (&descriptor.kind, &descriptor.value) {
        (FieldKind::Text, FieldValue::Text(text))
            if descriptor.id == "name" && !descriptor.read_only && alone =>
        {
            let text = text.clone();
            Box::new(bsn_list![row(bsn_list![
                field_label(&title),
                (
                    @FeathersTextInputContainer
                    Node { flex_grow: 1.0, min_width: px(0), border: UiRect::all(px(1)) }
                    ThemeBorderColor(BORDER)
                    Children [(
                        @FeathersTextInput { @max_characters: NAME_LENGTH }
                        NameInput({text})
                    )]
                )
            ])])
        }
        (FieldKind::Vector(4), FieldValue::Vector(values))
            if descriptor.id == "rotation" && !descriptor.read_only =>
        {
            let inputs = numbers(&descriptor.id, &euler_degrees(values), mixed);
            let title = format!("{name} ({})", wording::unit("degrees"));
            Box::new(bsn_list![row(bsn_list![field_label(&title), { inputs }])])
        }
        (FieldKind::Vector(3), FieldValue::Vector(values)) if !descriptor.read_only => {
            let inputs = numbers(&descriptor.id, values, mixed);
            Box::new(bsn_list![row(bsn_list![field_label(&title), { inputs }])])
        }
        (_, value) => {
            let text = shown_value(descriptor, value);
            Box::new(bsn_list![row(bsn_list![
                field_label(&title),
                label_dim(text)
            ])])
        }
    }
}

/// A rotation of the document, a quaternion as x, y, z, w, as turns in degrees about X, Y
/// and Z. Anything that is not four numbers is no rotation.
pub fn euler_degrees(quaternion: &[f32]) -> [f32; 3] {
    let &[x, y, z, w] = quaternion else {
        return [0.0; 3];
    };
    let (x, y, z) = Quat::from_xyzw(x, y, z, w).to_euler(EulerRot::XYZ);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

/// The quaternion of turns in degrees about X, Y and Z.
pub fn quaternion_of(degrees: [f32; 3]) -> [f32; 4] {
    Quat::from_euler(
        EulerRot::XYZ,
        degrees[0].to_radians(),
        degrees[1].to_radians(),
        degrees[2].to_radians(),
    )
    .normalize()
    .to_array()
}

/// A value that is shown and not edited here.
fn shown_value(descriptor: &FieldDescriptor, value: &FieldValue) -> String {
    if descriptor.mixed_selection {
        return MIXED.into();
    }
    match value {
        FieldValue::Text(text) => text.clone(),
        FieldValue::Number(number) => format!("{number:.3}"),
        FieldValue::Toggle(on) => if *on { "Yes" } else { "No" }.into(),
        // A rotation arrives as a quaternion; people read degrees.
        FieldValue::Vector(q) if descriptor.id == "rotation" && q.len() == 4 => {
            let (x, y, z) = Quat::from_xyzw(q[0], q[1], q[2], q[3]).to_euler(EulerRot::XYZ);
            format!(
                "{:.1}\u{b0}  {:.1}\u{b0}  {:.1}\u{b0}",
                x.to_degrees(),
                y.to_degrees(),
                z.to_degrees()
            )
        }
        FieldValue::Vector(values) => values
            .iter()
            .map(|value| format!("{value:.3}"))
            .collect::<Vec<_>>()
            .join("  "),
    }
}

/// What decides whether the panel must be rebuilt or only its values refreshed.
#[derive(PartialEq, Eq, Clone, Default)]
pub struct InspectorShape {
    session: u64,
    selection: Vec<String>,
    fields: Vec<String>,
    /// Which numbers are dashes: a number the objects come to share, or to differ in, changes
    /// what is built and not only what a field shows.
    mixed: Vec<[bool; 3]>,
    /// With several objects selected, what the panel says of the gizmo: the object it is on
    /// and whether it is at their middle. Nothing with one object or none.
    gizmo: Option<(Option<String>, bool)>,
}

fn shape(snapshot: &ViewSnapshot) -> InspectorShape {
    InspectorShape {
        session: snapshot.session,
        selection: snapshot.selection.iter().map(|id| id.0.clone()).collect(),
        fields: snapshot
            .inspector
            .iter()
            .map(|field| field.id.clone())
            .collect(),
        mixed: snapshot
            .inspector
            .iter()
            .map(|field| mixed_axes(snapshot, field))
            .collect(),
        gizmo: (snapshot.selection.len() > 1).then(|| {
            (
                gizmo_object(snapshot).map(|object| object.id.0.clone()),
                snapshot.pivot == GizmoPivot::Center,
            )
        }),
    }
}

/// Shows the selection's fields. Rebuilds when the selection or the set of fields changed;
/// otherwise only sends new values to the inputs, so that an input being used is not replaced.
pub(crate) fn show_inspector(
    mut commands: Commands,
    editor: NonSend<Editor>,
    body: Query<Entity, With<InspectorBody>>,
    mut inputs: Query<(Entity, &mut FieldInput)>,
    mut names: Query<(Entity, &mut NameInput)>,
    mut shown: Local<Option<(u64, InspectorShape)>>,
) {
    let Ok(body) = body.single() else {
        return;
    };
    let snapshot = editor.snapshot();
    let revision = snapshot.regions.inspector;
    let now = shape(&snapshot);
    match &*shown {
        Some((seen, before)) if *seen == revision && before.session == now.session => return,
        Some((_, before)) if *before == now => {
            for (entity, mut input) in &mut inputs {
                // A number the objects differ in has no value to show.
                if input.mixed {
                    continue;
                }
                let Some(value) = component(&snapshot, &input.field, input.axis).map(shown_number)
                else {
                    continue;
                };
                if value != input.value {
                    input.value = value;
                    commands.trigger(UpdateNumberInput {
                        entity,
                        value: NumberInputValue::F32(value),
                    });
                }
            }
            // The name changed without the field, by undo for instance: show it again.
            if let Some(FieldValue::Text(name)) = snapshot
                .inspector
                .iter()
                .find(|field| field.id == "name")
                .map(|field| &field.value)
            {
                for (_, mut input) in &mut names {
                    if input.0 != *name {
                        input.0.clone_from(name);
                    }
                }
            }
        }
        _ => {
            let mut scenes: Vec<Box<dyn SceneList>> = Vec::new();
            if snapshot.inspector.is_empty() {
                scenes.push(Box::new(bsn_list![label_dim(
                    "Select something to see and change its properties here."
                )]));
            }
            if let Some(count) = several(snapshot.selection.len()) {
                scenes.push(Box::new(bsn_list![
                    row(bsn_list![label(count)]),
                    label_dim(SEVERAL_HINT)
                ]));
                scenes.push(gizmo_rows(&snapshot));
            }
            let alone = snapshot.selection.len() == 1;
            scenes.extend(
                snapshot
                    .inspector
                    .iter()
                    .zip(&now.mixed)
                    .map(|(descriptor, mixed)| field(descriptor, *mixed, alone)),
            );
            commands
                .entity(body)
                .despawn_children()
                .queue_spawn_related_scenes::<Children>(scenes);
        }
    }
    *shown = Some((revision, now));
}

/// Writes the name into its field when the field is new or the name changed elsewhere, unless
/// the field is being typed in.
pub(crate) fn fill_names(
    mut names: Query<(Entity, &NameInput, &mut EditableText), Changed<NameInput>>,
    focus: Res<InputFocus>,
) {
    for (entity, name, mut text) in &mut names {
        if focus.get() == Some(entity) || text.value().to_string() == name.0 {
            continue;
        }
        text.queue_edit(TextEdit::SelectAll);
        text.queue_edit(TextEdit::Insert(name.0.as_str().into()));
    }
}

/// Enter in the name field renames the selected object. An empty or unchanged name does
/// nothing; a name the core refuses is reported like any other problem.
pub(crate) fn commit_name(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    mut names: Query<(&mut NameInput, &EditableText)>,
    mut editor: NonSendMut<Editor>,
) {
    if !keys.just_pressed(KeyCode::Enter) && !keys.just_pressed(KeyCode::NumpadEnter) {
        return;
    }
    let Some(Ok((mut shown, text))) = focus.get().map(|entity| names.get_mut(entity)) else {
        return;
    };
    let typed = text.value().to_string();
    let typed = typed.trim();
    let Some(object) = editor.snapshot().selection.first().cloned() else {
        return;
    };
    if typed.is_empty() || typed == shown.0 {
        return;
    }
    shown.0 = typed.to_owned();
    editor.rename(object, typed.to_owned());
}

/// Gives a new input its value.
pub(crate) fn fill_new_inputs(
    mut commands: Commands,
    inputs: Query<(Entity, &FieldInput), Added<FieldInput>>,
) {
    for (entity, input) in &inputs {
        // A number the objects differ in stays empty: its dash is what it shows.
        if input.mixed {
            continue;
        }
        commands.trigger(UpdateNumberInput {
            entity,
            value: NumberInputValue::F32(input.value),
        });
    }
}

fn component(snapshot: &ViewSnapshot, field: &str, axis: usize) -> Option<f32> {
    let descriptor = snapshot
        .inspector
        .iter()
        .find(|descriptor| descriptor.id == field)?;
    match &descriptor.value {
        FieldValue::Vector(values) if field == "rotation" => {
            euler_degrees(values).get(axis).copied()
        }
        FieldValue::Vector(values) => values.get(axis).copied(),
        FieldValue::Number(value) => Some(*value),
        _ => None,
    }
}

/// What a number typed into component `axis` of `field` changes: that number of every selected
/// object that has the field, and nothing else of it. An object keeps its other two numbers,
/// which may differ from its neighbours'.
pub fn edit_mutations(
    snapshot: &ViewSnapshot,
    field: &str,
    axis: usize,
    value: f32,
) -> Vec<Mutation> {
    snapshot
        .selection
        .iter()
        .filter_map(|id| snapshot.document.object(id))
        .filter_map(|object| {
            let mut numbers = numbers_of(object, field)?;
            numbers[axis % 3] = value;
            Some(match field {
                "translation" => Mutation::Transform {
                    object: object.id.clone(),
                    value: RigidTransform {
                        translation_m: numbers,
                        rotation_xyzw: object.local.rotation_xyzw,
                    },
                },
                "rotation" => Mutation::Transform {
                    object: object.id.clone(),
                    value: RigidTransform {
                        translation_m: object.local.translation_m,
                        rotation_xyzw: quaternion_of(numbers),
                    },
                },
                _ => Mutation::BoxExtents {
                    object: object.id.clone(),
                    value: numbers,
                },
            })
        })
        .collect()
}

/// What the core is asked for when a number is typed into component `axis` of `field`, where
/// it has a request of its own for that: one position along an axis, or one size along an
/// axis, for every selected object. The panel shows half sizes and the core takes whole ones.
/// `None` for a rotation, which is worked out object by object ([`edit_mutations`]).
pub fn assignment(field: &str, axis: usize, value: f32) -> Option<FieldAssignment> {
    let mut axes = [None; 3];
    match field {
        "translation" => {
            axes[axis % 3] = Some(value);
            Some(FieldAssignment::Translation(axes))
        }
        "half_extents" => {
            axes[axis % 3] = Some(value * 2.0);
            Some(FieldAssignment::Size(axes))
        }
        _ => None,
    }
}

/// Sends a committed number to the core as one edit of every selected object, checked against
/// the inspector's revision: a number typed for one selection never reaches another.
fn commit_field(
    change: On<ValueChange<f32>>,
    inputs: Query<&FieldInput>,
    mut editor: NonSendMut<Editor>,
) {
    if !change.is_final {
        return;
    }
    let Ok(input) = inputs.get(change.source) else {
        return;
    };
    // A number the objects differ in is new whatever it is: it has no value to equal.
    if !input.mixed && input.value == change.value {
        return;
    }
    if let Some(assignment) = assignment(&input.field, input.axis, change.value) {
        editor.assign(assignment);
        return;
    }
    let mutations = edit_mutations(&editor.snapshot(), &input.field, input.axis, change.value);
    if mutations.is_empty() {
        return;
    }
    editor.submit_from_inspector(Command::Edit {
        label_key: "edit".into(),
        mutations,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_snap_step_replaces_that_step_and_leaves_the_rest_of_the_setting() {
        let snap = SnapSettings {
            enabled: true,
            ..SnapSettings::default()
        };
        let finer = with_snap_step(snap, SNAP_GRID, 0.25);
        assert_eq!(
            (finer.enabled, finer.translation_m, finer.rotation_degrees),
            (true, 0.25, 15.0)
        );
        let coarser = with_snap_step(finer, SNAP_TURN, 45.0);
        assert_eq!(
            (coarser.translation_m, coarser.rotation_degrees),
            (0.25, 45.0)
        );
        assert_eq!(snap_step_of(coarser, SNAP_GRID), 0.25);
        assert_eq!(snap_step_of(coarser, SNAP_TURN), 45.0);
    }

    #[test]
    fn a_typed_position_or_size_is_one_request_for_all_and_a_size_is_asked_for_whole() {
        assert_eq!(
            assignment("translation", 1, 0.5),
            Some(FieldAssignment::Translation([None, Some(0.5), None]))
        );
        // The panel's field is half a size.
        assert_eq!(
            assignment("half_extents", 2, 1.5),
            Some(FieldAssignment::Size([None, None, Some(3.0)]))
        );
        assert_eq!(assignment("rotation", 0, 90.0), None);
    }

    #[test]
    fn the_panel_says_where_the_gizmo_of_several_objects_is() {
        let on_one = gizmo_line(GizmoPivot::Active, Some("Wall"), true);
        assert!(on_one.contains("on Wall") && on_one.contains("Shift-click"));
        let middle = gizmo_line(GizmoPivot::Center, Some("Wall"), true);
        assert!(middle.contains("middle") && !middle.contains("Wall"));
        // The middle was asked for, but only one of the objects can be moved by the gizmo:
        // it is on that one.
        let one_root = gizmo_line(GizmoPivot::Center, Some("Wall"), false);
        assert!(one_root.contains("on Wall"));
        assert!(gizmo_line(GizmoPivot::Active, None, false).contains("None of"));
    }

    #[test]
    fn the_gizmo_is_on_the_object_the_core_calls_active_and_moves_the_roots() {
        let snapshot = two_selected(|document| {
            // The second selected object is under the first.
            let above = document.objects[1].id.clone();
            document.objects[2].parent = Some(above);
        });
        let active = snapshot
            .active
            .clone()
            .expect("a selection has an active object");
        assert_eq!(
            gizmo_object(&snapshot).map(|object| &object.id),
            Some(&active)
        );
        // Of the two, only the upper one is moved by itself.
        let roots = gizmo_roots(&snapshot);
        assert_eq!(roots.len(), 1);
        assert!(!roots[0]
            .parent
            .as_ref()
            .is_some_and(|parent| snapshot.selection.contains(parent)));
    }

    #[test]
    fn a_number_is_shown_to_the_millimetre() {
        assert_eq!(shown_number(8.758_77), 8.759);
        assert_eq!(shown_number(-0.5), -0.5);
        assert_eq!(shown_number(3.0), 3.0);
        assert!(shown_number(-0.000_01).is_sign_positive());
    }

    #[test]
    fn degrees_and_the_documents_quaternion_convert_both_ways() {
        let quaternion = quaternion_of([10.0, -35.0, 20.0]);
        let length: f32 = quaternion.iter().map(|c| c * c).sum();
        assert!((length - 1.0).abs() < 1e-5);
        let degrees = euler_degrees(&quaternion);
        for (got, wanted) in degrees.iter().zip([10.0, -35.0, 20.0]) {
            assert!((got - wanted).abs() < 1e-3, "{degrees:?}");
        }
        assert_eq!(euler_degrees(&[0.0, 0.0]), [0.0; 3]);
    }

    #[test]
    fn a_rotation_is_shown_in_degrees_and_a_mixed_value_as_a_dash() {
        let quarter_turn = Quat::from_rotation_z(20.0_f32.to_radians());
        let mut descriptor = FieldDescriptor {
            id: "rotation".into(),
            label_key: "field.rotation".into(),
            unit_key: None,
            kind: FieldKind::Vector(4),
            value: FieldValue::Vector(quarter_turn.to_array().to_vec()),
            mixed_selection: false,
            read_only: false,
            min: None,
            max: None,
            step: None,
            validation: std::sync::Arc::from([]),
        };
        let shown = shown_value(&descriptor, &descriptor.value);
        assert!(shown.ends_with("20.0\u{b0}"), "{shown}");
        descriptor.mixed_selection = true;
        assert_eq!(shown_value(&descriptor, &descriptor.value), "\u{2014}");
    }

    /// The game's map with objects 1 and 2 selected, as the core shows it.
    fn two_selected(change: impl FnOnce(&mut scene_data::SceneDocument)) -> ViewSnapshot {
        let mut document = scene_data::current_map();
        change(&mut document);
        let selection = vec![
            document.objects[1].id.clone(),
            document.objects[2].id.clone(),
        ];
        let mut session = editor_core::EditorSession::new(document).unwrap();
        let before = session.snapshot();
        session
            .submit(editor_core::CommandRequest {
                request: 1,
                session: before.session,
                expected_revision: before.document_revision,
                expected_inspector_revision: None,
                command: Command::Select(selection),
            })
            .outcome
            .unwrap();
        (*session.snapshot()).clone()
    }

    fn descriptor<'a>(snapshot: &'a ViewSnapshot, field: &str) -> &'a FieldDescriptor {
        snapshot
            .inspector
            .iter()
            .find(|descriptor| descriptor.id == field)
            .expect("the field is shown")
    }

    #[test]
    fn only_the_numbers_the_selected_objects_differ_in_are_dashes() {
        // Two boxes on the same height and depth, three metres apart along X.
        let snapshot = two_selected(|document| {
            document.objects[1].local.translation_m = [1.0, 2.0, -4.0];
            document.objects[2].local.translation_m = [4.0, 2.0, -4.0];
        });
        let position = descriptor(&snapshot, "translation");
        assert!(
            position.mixed_selection,
            "the core says the positions differ"
        );
        assert_eq!(mixed_axes(&snapshot, position), [true, false, false]);
        assert_eq!(
            several(snapshot.selection.len()).as_deref(),
            Some("2 objects")
        );
        assert_eq!(several(1), None);
    }

    #[test]
    fn a_field_the_core_calls_the_same_has_no_dash_whatever_the_objects_are() {
        let snapshot = two_selected(|document| {
            let place = document.objects[1].local.clone();
            document.objects[2].local = place;
        });
        let position = descriptor(&snapshot, "translation");
        assert!(!position.mixed_selection);
        assert_eq!(mixed_axes(&snapshot, position), [false; 3]);
    }

    #[test]
    fn a_typed_number_goes_to_every_selected_object_and_leaves_their_other_numbers() {
        let snapshot = two_selected(|document| {
            document.objects[1].local.translation_m = [1.0, 2.0, -4.0];
            document.objects[2].local.translation_m = [4.0, 2.0, -4.0];
        });
        let mutations = edit_mutations(&snapshot, "translation", 1, 0.5);
        let moved: Vec<[f32; 3]> = mutations
            .iter()
            .map(|mutation| match mutation {
                Mutation::Transform { value, .. } => value.translation_m,
                other => panic!("expected a transform, got {other:?}"),
            })
            .collect();
        assert_eq!(moved, [[1.0, 0.5, -4.0], [4.0, 0.5, -4.0]]);
    }
}
