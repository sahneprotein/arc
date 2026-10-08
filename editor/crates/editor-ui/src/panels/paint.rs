//! The paint part of the landscape tool: the landscape's layers.
//!
//! A layer is a named surface of the ground with a colour. The list shows the layers of the
//! landscape the tools work on; a click chooses the one to paint. Adding, renaming and
//! removing a layer sends the landscape's whole list of layers to the core as one undo step;
//! the core keeps what is painted on the layers that stay.

use bevy::{
    feathers::{
        controls::{ButtonVariant, FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        display::{label, label_dim},
        theme::{ThemeBorderColor, ThemedText},
    },
    input_focus::InputFocus,
    prelude::*,
    text::{EditableText, TextEdit},
    ui_widgets::Activate,
};
use scene_data::terrain::LayerDefinition;

use crate::{
    session::{filled_by_rule, landscape, layer_average, Editor},
    theme::{feathers::BORDER, size, spacing},
    tool::{PaintLayer, Tool},
};

/// The longest name a layer takes in the field.
const NAME_LENGTH: usize = 48;

/// The list of the landscape's layers.
#[derive(Component, Default, Clone)]
pub struct LayerList;

/// The button of one layer, and the layer's stable ID.
#[derive(Component, Default, Clone)]
pub struct LayerRow(pub String);

/// The note at the end of a layer's row, and the layer's stable ID. It changes with every
/// stroke, so it is a text of its own: the rows are not built anew for it.
#[derive(Component, Default, Clone)]
pub struct LayerNote(pub String);

/// The field of the chosen layer's name, and the name it was last given to show.
#[derive(Component, Default, Clone)]
pub struct LayerNameInput(pub String);

/// What a layer's row says after its name: its average weight, or that a rule fills it.
pub fn layer_note(average: Option<f32>) -> String {
    match average {
        Some(average) => format!("{:.0} %", average * 100.0),
        None => "by rule".to_owned(),
    }
}

fn layer_row(
    number: usize,
    layer: &LayerDefinition,
    note: &str,
    chosen: bool,
) -> impl Scene + use<> {
    let id = layer.id.0.clone();
    let note_of = id.clone();
    let note = note.to_owned();
    let caption = format!("{number}  {}", layer.name);
    // A mask has no colour of its own; its swatch is an empty frame.
    let color = layer
        .color_linear_rgba
        .map_or(Color::NONE, |[r, g, b, a]| Color::linear_rgba(r, g, b, a));
    let variant = if chosen {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_2),
            flex_shrink: 0.0,
        }
        Children [
            (
                Node {
                    width: px(size::ROW_HEIGHT - 8.0),
                    height: px(size::ROW_HEIGHT - 8.0),
                    flex_shrink: 0.0,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(2)),
                }
                BackgroundColor({color})
                ThemeBorderColor(BORDER)
            ),
            (
                @FeathersButton { @caption: bsn! { Text({caption}) ThemedText }, @variant: {variant} }
                LayerRow({id})
                Node { flex_grow: 1.0, justify_content: JustifyContent::Start }
                on(|activate: On<Activate>, rows: Query<&LayerRow>, mut chosen: ResMut<PaintLayer>| {
                    if let Ok(row) = rows.get(activate.entity) {
                        chosen.0 = Some(row.0.clone());
                    }
                })
            ),
            (
                Node {
                    width: px(56),
                    flex_shrink: 0.0,
                    justify_content: JustifyContent::End,
                    align_items: AlignItems::Center,
                }
                Children [ (label_dim(note) LayerNote({note_of})) ]
            ),
        ]
    }
}

/// The paint part, to be placed in the landscape tool's panel.
pub fn paint_part() -> impl Scene {
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(spacing::SPACE_2), flex_shrink: 0.0 }
        Children [
            label_dim("Layers"),
            (
                LayerList
                Node { flex_direction: FlexDirection::Column, row_gap: px(spacing::SPACE_1) }
            ),
            (
                Node { flex_direction: FlexDirection::Row, column_gap: px(spacing::SPACE_2) }
                Children [
                    (
                        @FeathersButton { @caption: bsn! { Text("Add layer") ThemedText } }
                        Node { flex_grow: 1.0, flex_basis: px(0) }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>, mut chosen: ResMut<PaintLayer>| {
                            if let Some(layer) = editor.add_layer() {
                                chosen.0 = Some(layer);
                            }
                        })
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Remove layer") ThemedText } }
                        Node { flex_grow: 1.0, flex_basis: px(0) }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>, chosen: Res<PaintLayer>| {
                            if let Some(layer) = &chosen.0 {
                                editor.remove_layer(layer);
                            }
                        })
                    ),
                ]
            ),
            (
                Node {
                    min_height: px(size::ROW_HEIGHT),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(spacing::SPACE_4),
                }
                Children [
                    (Node { width: px(96), flex_shrink: 0.0 } Children [ label("Layer name") ]),
                    (
                        @FeathersTextInputContainer
                        Node { flex_grow: 1.0, min_width: px(0), border: UiRect::all(px(1)) }
                        ThemeBorderColor(BORDER)
                        Children [(
                            @FeathersTextInput { @max_characters: NAME_LENGTH }
                            LayerNameInput
                        )]
                    ),
                ]
            ),
        ]
    }
}

/// What the rows were last built from: the core's session, the landscape, the chosen layer,
/// and each layer's ID, name and colour. A stroke changes none of these.
type RowsShown = (
    u64,
    Option<String>,
    Option<String>,
    Vec<(String, String, Option<[u32; 4]>)>,
);

/// Shows the layers of the landscape while the paint tool is chosen, and keeps the chosen
/// layer one that exists.
pub(crate) fn show_layers(
    mut commands: Commands,
    editor: NonSend<Editor>,
    (tool, mut chosen): (Res<Tool>, ResMut<PaintLayer>),
    list: Query<Entity, With<LayerList>>,
    mut name: Query<&mut LayerNameInput>,
    mut notes: Query<(&LayerNote, &mut Text)>,
    mut shown: Local<Option<(RowsShown, u64)>>,
) {
    if *tool != Tool::Paint {
        return;
    }
    let Ok(list) = list.single() else {
        return;
    };
    let snapshot = editor.snapshot();
    let found = landscape(&snapshot);
    let layers: &[LayerDefinition] = found.map_or(&[], |(_, terrain)| &terrain.layers);
    let exists = |id: &String| layers.iter().any(|layer| &layer.id.0 == id);
    if !chosen.0.as_ref().is_some_and(exists) {
        // A layer just added is chosen before the core has it: wait for the core's answer
        // instead of falling back to the first layer.
        if chosen.0.is_some() && editor.busy() {
            return;
        }
        let first = layers.first().map(|layer| layer.id.0.clone());
        if chosen.0 != first {
            chosen.0 = first;
        }
    }
    let key: RowsShown = (
        snapshot.session,
        found.map(|(object, _)| object.id.0.clone()),
        chosen.0.clone(),
        layers
            .iter()
            .map(|layer| {
                let color = layer.color_linear_rgba.map(|color| color.map(f32::to_bits));
                (layer.id.0.clone(), layer.name.clone(), color)
            })
            .collect(),
    );
    match shown.as_ref() {
        Some((rows, revision)) if *rows == key => {
            // The same rows: only what is painted may have changed.
            if *revision != snapshot.document_revision
                && let Some((_, terrain)) = found
            {
                for (note, mut text) in &mut notes {
                    let Some(layer) = layers.iter().find(|layer| layer.id.0 == note.0) else {
                        continue;
                    };
                    let now = layer_note(layer_average(terrain, layer));
                    if text.0 != now {
                        text.0 = now;
                    }
                }
                *shown = Some((key, snapshot.document_revision));
            }
            return;
        }
        _ => {}
    }
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    match found {
        None => rows.push(Box::new(bsn_list![label_dim(
            "The level has no landscape. Add one to give it layers."
        )])),
        Some(_) if layers.is_empty() => rows.push(Box::new(bsn_list![label_dim(
            "No layers yet. The first one covers the whole ground."
        )])),
        Some((_, terrain)) => {
            for (index, layer) in layers.iter().enumerate() {
                let note = layer_note(layer_average(terrain, layer));
                let is_chosen = chosen.0.as_deref() == Some(layer.id.0.as_str());
                rows.push(Box::new(bsn_list![layer_row(
                    index + 1,
                    layer,
                    &note,
                    is_chosen
                )]));
            }
        }
    }
    commands
        .entity(list)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(rows);
    let chosen_name = layers
        .iter()
        .find(|layer| chosen.0.as_deref() == Some(layer.id.0.as_str()))
        .map(|layer| layer.name.clone())
        .unwrap_or_default();
    for mut input in &mut name {
        if input.0 != chosen_name {
            input.0.clone_from(&chosen_name);
        }
    }
    *shown = Some((key, snapshot.document_revision));
}

/// Writes the chosen layer's name into its field, unless the field is being typed in.
pub(crate) fn fill_layer_names(
    mut names: Query<(Entity, &LayerNameInput, &mut EditableText), Changed<LayerNameInput>>,
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

/// Enter in the layer's name field renames the chosen layer. An empty or unchanged name does
/// nothing; a name the core refuses is reported like any other problem.
pub(crate) fn commit_layer_name(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    chosen: Res<PaintLayer>,
    mut names: Query<(&mut LayerNameInput, &EditableText)>,
    mut editor: NonSendMut<Editor>,
) {
    if !keys.just_pressed(KeyCode::Enter) && !keys.just_pressed(KeyCode::NumpadEnter) {
        return;
    }
    let Some(Ok((mut shown, text))) = focus.get().map(|entity| names.get_mut(entity)) else {
        return;
    };
    let Some(layer) = &chosen.0 else {
        return;
    };
    let typed = text.value().to_string();
    let typed = typed.trim();
    if typed.is_empty() || typed == shown.0 {
        return;
    }
    shown.0 = typed.to_owned();
    editor.rename_layer(layer, typed.to_owned());
}

/// Whether the layer can be painted: not one a rule fills.
pub fn paintable(editor: &Editor, layer: &str) -> bool {
    let snapshot = editor.snapshot();
    landscape(&snapshot).is_some_and(|(_, terrain)| {
        terrain
            .layers
            .iter()
            .find(|definition| definition.id.0 == layer)
            .is_some_and(|definition| !filled_by_rule(terrain, definition))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_says_the_average_weight_or_that_a_rule_fills_the_layer() {
        assert_eq!(layer_note(Some(1.0)), "100 %");
        assert_eq!(layer_note(Some(0.274)), "27 %");
        assert_eq!(layer_note(None), "by rule");
    }
}
