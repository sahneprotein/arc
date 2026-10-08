//! The rules part of the landscape tool: what grows where, and which layers fill themselves.
//!
//! The list shows the landscape's rules in the level's order, each described by what it does,
//! since a rule has no name. A click chooses the rule whose settings show underneath. A rule
//! reads and fills layers; each such place is a button that steps to the next layer that can
//! go there. Every change sends the landscape's whole rule list to the core as one undo step.
//! What a rule makes of the ground is the core's work and arrives with the level's geometry.

use bevy::{
    feathers::{
        controls::{
            ButtonVariant, FeathersButton, FeathersNumberInput, NumberInputValue, UpdateNumberInput,
        },
        display::{label, label_dim},
        theme::ThemedText,
    },
    prelude::*,
    ui_widgets::{Activate, ValueChange},
};
use scene_data::terrain::{LayerDefinition, RuleDefinition};

use crate::{
    rules::{self, Setting, GRASS, SLOPE},
    session::{landscape, Editor, RulePlace},
    theme::{size, spacing},
    tool::{ChosenRule, Tool},
};

/// The list of the landscape's rules.
#[derive(Component, Default, Clone)]
pub struct RuleList;

/// The button of one rule, and the rule's stable ID.
#[derive(Component, Default, Clone)]
pub struct RuleRow(pub String);

/// The node the chosen rule's settings are built in.
#[derive(Component, Default, Clone)]
pub struct RuleSettings;

/// The number field of one setting of the chosen rule: the setting's place in its kind's
/// list, and the value the field shows.
#[derive(Component, Default, Clone)]
pub struct RuleField {
    pub setting: usize,
    pub value: f32,
}

/// A button that steps one of the chosen rule's layers on to the next.
#[derive(Component, Default, Clone)]
pub struct RuleLayerButton(pub RulePlace);

fn rule_row(
    number: usize,
    rule: &RuleDefinition,
    layers: &[LayerDefinition],
    chosen: bool,
) -> impl Scene + use<> {
    let id = rule.id.0.clone();
    let caption = format!("{number}  {}", rules::describe(rule, layers));
    let variant = if chosen {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    bsn! {
        @FeathersButton { @caption: bsn! { Text({caption}) ThemedText }, @variant: {variant} }
        RuleRow({id})
        Node { justify_content: JustifyContent::Start, flex_shrink: 0.0 }
        on(|activate: On<Activate>, rows: Query<&RuleRow>, mut chosen: ResMut<ChosenRule>| {
            if let Ok(row) = rows.get(activate.entity) {
                chosen.0 = Some(row.0.clone());
            }
        })
    }
}

/// The rules part, to be placed in the landscape tool's panel.
pub fn rules_part() -> impl Scene {
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(spacing::SPACE_2), flex_shrink: 0.0 }
        Children [
            label_dim("Rules"),
            (
                RuleList
                Node { flex_direction: FlexDirection::Column, row_gap: px(spacing::SPACE_1) }
            ),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(spacing::SPACE_2),
                    row_gap: px(spacing::SPACE_2),
                }
                Children [
                    (
                        @FeathersButton { @caption: bsn! { Text("Add grass") ThemedText } }
                        Node { flex_grow: 1.0 }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>, mut chosen: ResMut<ChosenRule>| {
                            if let Some(rule) = editor.add_grass_rule() {
                                chosen.0 = Some(rule);
                            }
                        })
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Add steep layer") ThemedText } }
                        Node { flex_grow: 1.0 }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>, mut chosen: ResMut<ChosenRule>| {
                            if let Some(rule) = editor.add_slope_rule() {
                                chosen.0 = Some(rule);
                            }
                        })
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Remove rule") ThemedText } }
                        Node { flex_grow: 1.0 }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>, chosen: Res<ChosenRule>| {
                            if let Some(rule) = &chosen.0 {
                                editor.remove_rule(rule);
                            }
                        })
                    ),
                ]
            ),
            (
                RuleSettings
                Node { flex_direction: FlexDirection::Column, row_gap: px(spacing::SPACE_2) }
            ),
            label_dim("Rules only change how the level looks. They never change what can be walked on."),
        ]
    }
}

fn setting_row(index: usize, setting: &Setting, value: f32) -> impl Scene + use<> {
    let title = setting.label;
    bsn! {
        Node {
            min_height: px(size::ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            flex_shrink: 0.0,
        }
        Children [
            (Node { width: px(136), flex_shrink: 0.0 } Children [ label(title) ]),
            (
                @FeathersNumberInput
                RuleField { setting: {index}, value: {value} }
                Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0) }
                on(commit_rule_field)
            ),
        ]
    }
}

fn layer_row(title: &'static str, place: RulePlace, shown: String) -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            flex_shrink: 0.0,
        }
        Children [
            (Node { width: px(136), flex_shrink: 0.0 } Children [ label(title) ]),
            (
                @FeathersButton { @caption: bsn! { Text({shown}) ThemedText } }
                RuleLayerButton({place})
                Node { flex_grow: 1.0, justify_content: JustifyContent::Start }
                on(|activate: On<Activate>,
                    buttons: Query<&RuleLayerButton>,
                    chosen: Res<ChosenRule>,
                    mut editor: NonSendMut<Editor>| {
                    if let (Ok(button), Some(rule)) = (buttons.get(activate.entity), &chosen.0) {
                        editor.step_rule_layer(rule, button.0);
                    }
                })
            ),
        ]
    }
}

/// A value as a setting's field shows it: three decimals.
fn shown(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0 + 0.0
}

/// Sends a committed number to the core as the chosen rule's new setting.
fn commit_rule_field(
    change: On<ValueChange<f32>>,
    fields: Query<&RuleField>,
    chosen: Res<ChosenRule>,
    mut editor: NonSendMut<Editor>,
) {
    if !change.is_final {
        return;
    }
    let (Ok(field), Some(rule)) = (fields.get(change.source), &chosen.0) else {
        return;
    };
    if field.value == change.value {
        return;
    }
    let snapshot = editor.snapshot();
    let Some(setting) = landscape(&snapshot)
        .and_then(|(_, terrain)| terrain.rules.iter().find(|other| &other.id.0 == rule))
        .and_then(|rule| rules::settings(&rule.algorithm).get(field.setting))
    else {
        return;
    };
    editor.set_rule_setting(rule, setting, change.value);
}

fn layer_name(
    layers: &[LayerDefinition],
    layer: Option<&scene_data::LayerId>,
    none: &str,
) -> String {
    layer
        .and_then(|layer| layers.iter().find(|definition| &definition.id == layer))
        .map_or_else(|| none.to_owned(), |definition| definition.name.clone())
}

/// The settings of one rule: its layers, its numbers, and what to know about them.
fn settings_of(rule: &RuleDefinition, layers: &[LayerDefinition]) -> Vec<Box<dyn SceneList>> {
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    match rule.algorithm.as_str() {
        GRASS => {
            let grows_on = layer_name(layers, rule.density_layer.as_ref(), "Everywhere");
            let kept_out = layer_name(layers, rule.exclusion_layers.first(), "Nothing");
            rows.push(Box::new(bsn_list![
                layer_row("Grows on", RulePlace::GrowsOn, grows_on),
                layer_row("Kept out by", RulePlace::KeptOutBy, kept_out),
            ]));
        }
        SLOPE => {
            let fills = layer_name(layers, rule.output_layer.as_ref(), "No layer");
            rows.push(Box::new(bsn_list![layer_row(
                "Fills layer",
                RulePlace::Fills,
                fills
            )]));
        }
        _ => rows.push(Box::new(bsn_list![label_dim(
            "This rule was made by another version of the editor. It is kept as it is."
        )])),
    }
    for (index, setting) in rules::settings(&rule.algorithm).iter().enumerate() {
        let value = shown(rules::value(rule, setting));
        rows.push(Box::new(bsn_list![setting_row(index, setting, value)]));
    }
    match rule.algorithm.as_str() {
        GRASS => rows.push(Box::new(bsn_list![label_dim(
            "Grass grows where its layer is, as thick as the layer is strong, and stays out of \
             the layer that keeps it out. Tries per cell is how many blades each cell of the \
             ground may get."
        )])),
        SLOPE => {
            rows.push(Box::new(bsn_list![label_dim(
                "Steepness is 0 on flat ground and 1 on a vertical face. The layer is filled \
                 between From and To and fades in and out over the fade. Painting it is \
                 refused while this rule fills it."
            )]));
            if let Some(note) = rules::flat_ground_note(rule) {
                rows.push(Box::new(bsn_list![label(note)]));
            }
        }
        _ => {}
    }
    rows
}

/// What the rule list and the settings were last built from: the core's session, the
/// landscape, the chosen rule, each rule's ID and description, and the note about flat ground.
type RulesShown = (
    u64,
    Option<String>,
    Option<String>,
    Vec<(String, String)>,
    Option<&'static str>,
);

/// Shows the landscape's rules while the rules tool is chosen, and keeps the chosen rule one
/// that exists. The lists are built anew only when a rule comes, goes or changes what it
/// reads or fills; a changed number is written into its field.
pub(crate) fn show_rules(
    mut commands: Commands,
    editor: NonSend<Editor>,
    (tool, mut chosen): (Res<Tool>, ResMut<ChosenRule>),
    list: Query<Entity, With<RuleList>>,
    settings: Query<Entity, With<RuleSettings>>,
    mut fields: Query<(Entity, &mut RuleField)>,
    mut shown_before: Local<Option<(RulesShown, u64)>>,
) {
    if *tool != Tool::Rules {
        return;
    }
    let (Ok(list), Ok(settings)) = (list.single(), settings.single()) else {
        return;
    };
    let snapshot = editor.snapshot();
    let found = landscape(&snapshot);
    let (rules_now, layers): (&[RuleDefinition], &[LayerDefinition]) =
        found.map_or((&[], &[]), |(_, terrain)| (&terrain.rules, &terrain.layers));
    let exists = |id: &String| rules_now.iter().any(|rule| &rule.id.0 == id);
    if !chosen.0.as_ref().is_some_and(exists) {
        // A rule just added is chosen before the core has it: wait for the core's answer.
        if chosen.0.is_some() && editor.busy() {
            return;
        }
        let first = rules_now.first().map(|rule| rule.id.0.clone());
        if chosen.0 != first {
            chosen.0 = first;
        }
    }
    let chosen_rule = rules_now
        .iter()
        .find(|rule| chosen.0.as_deref() == Some(rule.id.0.as_str()));
    let key: RulesShown = (
        snapshot.session,
        found.map(|(object, _)| object.id.0.clone()),
        chosen.0.clone(),
        rules_now
            .iter()
            .map(|rule| {
                // The description, and the layers in the rule's places, by name.
                let places = format!(
                    "{}|{}|{}",
                    layer_name(layers, rule.density_layer.as_ref(), ""),
                    layer_name(layers, rule.exclusion_layers.first(), ""),
                    layer_name(layers, rule.output_layer.as_ref(), ""),
                );
                (
                    rule.id.0.clone(),
                    format!("{}|{places}", rules::describe(rule, layers)),
                )
            })
            .collect(),
        chosen_rule.and_then(rules::flat_ground_note),
    );
    if let Some((before, revision)) = shown_before.as_ref()
        && *before == key
    {
        // The same rows: only numbers may have changed.
        if *revision != snapshot.document_revision
            && let Some(rule) = chosen_rule
        {
            let all = rules::settings(&rule.algorithm);
            for (entity, mut field) in &mut fields {
                let Some(setting) = all.get(field.setting) else {
                    continue;
                };
                let value = shown(rules::value(rule, setting));
                if value != field.value {
                    field.value = value;
                    commands.trigger(UpdateNumberInput {
                        entity,
                        value: NumberInputValue::F32(value),
                    });
                }
            }
            *shown_before = Some((key, snapshot.document_revision));
        }
        return;
    }
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    match found {
        None => rows.push(Box::new(bsn_list![label_dim(
            "The level has no landscape. Rules belong to a landscape."
        )])),
        Some(_) if rules_now.is_empty() => rows.push(Box::new(bsn_list![label_dim(
            "No rules yet. Add grass, or a layer that fills itself on steep ground."
        )])),
        Some(_) => {
            for (index, rule) in rules_now.iter().enumerate() {
                let is_chosen = chosen.0.as_deref() == Some(rule.id.0.as_str());
                rows.push(Box::new(bsn_list![rule_row(
                    index + 1,
                    rule,
                    layers,
                    is_chosen
                )]));
            }
        }
    }
    commands
        .entity(list)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(rows);
    let rows = chosen_rule.map_or_else(Vec::new, |rule| settings_of(rule, layers));
    commands
        .entity(settings)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(rows);
    *shown_before = Some((key, snapshot.document_revision));
}

/// Gives a new field its value.
pub(crate) fn fill_rule_fields(
    mut commands: Commands,
    fields: Query<(Entity, &RuleField), Added<RuleField>>,
) {
    for (entity, field) in &fields {
        commands.trigger(UpdateNumberInput {
            entity,
            value: NumberInputValue::F32(field.value),
        });
    }
}
