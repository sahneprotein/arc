//! The emitters' list: the game's particle emitters, in the hierarchy's place while the
//! Particles tool is chosen.
//!
//! Each row is one file of the game's `assets/particles` folder, or an emitter that was made
//! here and has no file yet. A row says in words when its emitter has changes that are not
//! saved, and when its file cannot be read; such a file is listed all the same, since hiding
//! it would hide why an emitter is missing. Choosing a row shows that emitter's settings and
//! its preview.
//!
//! Under the list are the three things done to whole emitters: a new one, a copy of the
//! chosen one, and removing the chosen one. Removing takes the emitter's file out of the
//! game's folder and cannot be undone, so it is asked about first, in a dialog.
//!
//! Without a game there is nowhere to keep emitters, and the list says so.

use bevy::{
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::{ButtonVariant, FeathersButton, FeathersListRow, FeathersListView},
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemedText},
    },
    prelude::*,
    ui::{InteractionDisabled, Selected},
    ui_widgets::{Activate, ValueChange},
};

use crate::{
    particles::{EmitterRow, Particles},
    shell::PanelChoice,
    theme::{
        feathers::{BORDER, SCRIM, SIGNAL_WARN},
        radius, size, spacing,
    },
    tool::Tool,
    wording,
};

/// The emitters' list panel.
#[derive(Component, Default, Clone)]
pub struct EmittersPanel;

/// The node the rows are rebuilt in.
#[derive(Component, Default, Clone)]
pub(crate) struct EmittersBody;

/// The row of one emitter, and the emitter's ID.
#[derive(Component, Default, Clone)]
pub struct EmitterRowMark(pub String);

/// The line of a row whose file cannot be read: why, in the warning colour.
#[derive(Component, Default, Clone)]
pub struct EmitterRowReason;

/// What one of the buttons under the list does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EmitterAction {
    #[default]
    New,
    Duplicate,
    Remove,
}

/// One of the buttons under the list.
#[derive(Component, Default, Clone)]
pub struct EmitterActionButton(pub EmitterAction);

/// What the list says without a game, and with a game that has no emitters.
pub const NO_GAME: &str = "Emitters are kept in the game's folder. Start the editor with \
    --game and the game's folder.";
pub const NO_EMITTERS: &str = "The game has no emitters yet. New emitter makes one.";

/// What a row says of its emitter with the name, and whether that is a warning: nothing
/// while the emitter is as its file is, that it has changes that are not saved, or, of a file
/// that cannot be read, the reason. The chosen row's panel says the rest: what to do, and
/// where the file is.
pub fn row_state(row: &EmitterRow) -> (String, bool) {
    match (&row.refusal, row.unsaved) {
        (Some((code, field)), _) => {
            let field = |key: &str| {
                if key == "field" {
                    field.as_deref()
                } else {
                    None
                }
            };
            let reason = wording::emitter(code, field, None);
            (reason.title, true)
        }
        (None, true) => ("Unsaved".into(), false),
        (None, false) => (String::new(), false),
    }
}

fn act(
    activate: On<Activate>,
    actions: Query<&EmitterActionButton>,
    mut particles: ResMut<Particles>,
) {
    match actions.get(activate.entity).map(|button| button.0) {
        Ok(EmitterAction::New) => {
            particles.new_emitter();
        }
        Ok(EmitterAction::Duplicate) => {
            particles.duplicate_chosen();
        }
        Ok(EmitterAction::Remove) => particles.ask_remove(),
        Err(_) => {}
    }
}

fn action_button(caption: &'static str, action: EmitterAction) -> impl Scene {
    bsn! {
        @FeathersButton { @caption: bsn! { Text(caption) ThemedText } }
        EmitterActionButton({action})
        Node { flex_grow: 1.0 }
        on(act)
    }
}

pub fn emitters_panel() -> impl Scene {
    bsn! {
        pane()
        EmittersPanel
        Node { width: px(size::HIERARCHY_WIDTH), flex_shrink: 0.0, display: Display::None }
        Children [
            (pane_header() Children [ (Text("Emitters") ThemedText) ]),
            (
                pane_body()
                EmittersBody
                Node { flex_grow: 1.0, min_height: px(0), padding: px(0) }
            ),
            (
                pane_body()
                Node {
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(spacing::SPACE_2),
                    row_gap: px(spacing::SPACE_2),
                    border: UiRect::top(px(1)),
                }
                ThemeBorderColor(BORDER)
                Children [
                    action_button("New emitter", EmitterAction::New),
                    action_button("Duplicate emitter", EmitterAction::Duplicate),
                    action_button("Remove emitter", EmitterAction::Remove),
                ]
            ),
        ]
    }
}

fn row(row: &EmitterRow) -> impl Scene + use<> {
    let id = row.id.clone();
    let name = row.name.clone();
    let (state, warning) = row_state(row);
    // A reason is a sentence and goes under the file's name, in the warning colour, which
    // always comes with these words. "Unsaved" is one word and goes beside the name.
    let said: Box<dyn SceneList> = if warning {
        Box::new(bsn_list![(
            label(state)
            ThemeTextColor(SIGNAL_WARN)
            EmitterRowReason
        )])
    } else {
        Box::new(bsn_list![Node { flex_grow: 1.0 }, label_dim(state)])
    };
    let (direction, gap) = if warning {
        (FlexDirection::Column, spacing::SPACE_1)
    } else {
        (FlexDirection::Row, spacing::SPACE_4)
    };
    bsn! {
        @FeathersListRow
        EmitterRowMark({id})
        Node {
            flex_direction: {direction},
            column_gap: px(gap),
            row_gap: px(gap),
            padding: UiRect::axes(px(spacing::SPACE_4), px(spacing::SPACE_1)),
        }
        Children [
            (Text({name}) ThemedText),
            {said},
        ]
    }
}

/// The list's word that a row was chosen, by a click or with the arrow keys.
fn choose_row(
    change: On<ValueChange<Entity>>,
    rows: Query<&EmitterRowMark>,
    mut particles: ResMut<Particles>,
) {
    if let Ok(row) = rows.get(change.value)
        && particles.chosen_id() != Some(row.0.as_str())
    {
        particles.choose(&row.0);
    }
}

fn list(rows: Vec<impl Scene>) -> impl Scene {
    let rows: Box<dyn SceneList> = Box::new(rows);
    bsn! {
        @FeathersListView { @rows: {rows} }
        Node { flex_grow: 1.0, min_height: px(0) }
        on(choose_row)
    }
}

fn note(words: &'static str) -> impl Scene {
    bsn! {
        Node { padding: UiRect::all(px(spacing::SPACE_4)) }
        Children [ label_dim(words) ]
    }
}

/// What the rows were last built from: whether there is a game, and each row without the
/// choice, which is shown by a mark and needs no new rows.
type RowsShown = (bool, Vec<(String, String, String)>);

/// Rebuilds the list when the emitters, their names or what a row says of its emitter
/// changed, and marks the chosen row.
pub(crate) fn show_emitters(
    mut commands: Commands,
    particles: Res<Particles>,
    body: Query<Entity, With<EmittersBody>>,
    marks: Query<(Entity, &EmitterRowMark, Has<Selected>)>,
    actions: Query<(Entity, &EmitterActionButton, Has<InteractionDisabled>)>,
    mut shown: Local<Option<RowsShown>>,
) {
    let Ok(body) = body.single() else {
        return;
    };
    let rows = particles.rows();
    let now: RowsShown = (
        particles.has_game(),
        rows.iter()
            .map(|row| (row.id.clone(), row.name.clone(), row_state(row).0))
            .collect(),
    );
    if shown.as_ref() != Some(&now) {
        let scenes: Vec<Box<dyn SceneList>> = vec![if !particles.has_game() {
            Box::new(bsn_list![note(NO_GAME)])
        } else if rows.is_empty() {
            Box::new(bsn_list![note(NO_EMITTERS)])
        } else {
            Box::new(bsn_list![list(rows.iter().map(row).collect())])
        }];
        commands
            .entity(body)
            .despawn_children()
            .queue_spawn_related_scenes::<Children>(scenes);
        *shown = Some(now);
    }
    // Compared each frame and written only on a change: rows may be made after the choice.
    for (entity, mark, marked) in &marks {
        let chosen = particles.chosen_id() == Some(mark.0.as_str());
        if chosen && !marked {
            commands.entity(entity).insert(Selected);
        } else if !chosen && marked {
            commands.entity(entity).remove::<Selected>();
        }
    }
    for (entity, action, off) in &actions {
        let offered = match action.0 {
            EmitterAction::New => particles.has_game(),
            EmitterAction::Duplicate | EmitterAction::Remove => particles.can_remove(),
        };
        if offered && off {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else if !offered && !off {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
}

/// Shows the list in the hierarchy's place while the Particles tool is chosen and that
/// place is shown.
pub(crate) fn fit_emitters(
    tool: Res<Tool>,
    choice: Res<PanelChoice>,
    mut panels: Query<&mut Node, With<EmittersPanel>>,
) {
    let wanted = if choice.shown().0 && *tool == Tool::Particles {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut panels {
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// The dialog that asks before an emitter is removed, and the layer behind it.
#[derive(Component, Default, Clone)]
pub struct RemoveDialog;

/// The dialog's question, which names the emitter, and the line under it.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, FromTemplate)]
pub enum RemoveLine {
    #[default]
    Question,
    Consequence,
}

/// What the dialog asks and what it says follows, for an emitter of this name that has a
/// file or has none yet.
pub fn remove_words(name: &str, has_file: bool) -> (String, &'static str) {
    (
        format!("Remove the emitter \u{201c}{name}\u{201d}?"),
        if has_file {
            "Its file is removed from the game's folder. Anything in the game that uses the \
             emitter will miss it. This cannot be undone."
        } else {
            "It was never saved, so no file is removed. This cannot be undone."
        },
    )
}

pub fn remove_dialog() -> impl Scene {
    bsn! {
        RemoveDialog
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            display: Display::None,
        }
        // In front of everything, and it takes every click: nothing behind can be used.
        GlobalZIndex(110)
        ThemeBackgroundColor(SCRIM)
        Children [(
            pane_body()
            Node {
                max_width: px(420),
                row_gap: px(spacing::SPACE_5),
                padding: UiRect::all(px(spacing::SPACE_6)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(radius::RADIUS_FIELD)),
            }
            ThemeBorderColor(BORDER)
            Children [
                (label("") RemoveLine::Question),
                (label_dim("") RemoveLine::Consequence),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::End,
                        column_gap: px(spacing::SPACE_4),
                        row_gap: px(spacing::SPACE_2),
                    }
                    Children [
                        (
                            @FeathersButton { @caption: bsn! { Text("Keep emitter") ThemedText } }
                            on(|_: On<Activate>, mut particles: ResMut<Particles>| {
                                particles.cancel_remove();
                            })
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text("Remove it") ThemedText },
                                @variant: ButtonVariant::Primary,
                            }
                            on(|_: On<Activate>, mut particles: ResMut<Particles>| {
                                particles.confirm_remove();
                            })
                        ),
                    ]
                ),
            ]
        )]
    }
}

/// Shows the dialog while a removal is asked about, with the emitter's name.
pub(crate) fn show_remove_dialog(
    particles: Res<Particles>,
    mut dialog: Query<&mut Node, With<RemoveDialog>>,
    mut lines: Query<(&RemoveLine, &mut Text)>,
) {
    let asked = particles.removing();
    let wanted = if asked.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut dialog {
        if node.display != wanted {
            node.display = wanted;
        }
    }
    let Some((name, has_file)) = asked else {
        return;
    };
    let (question, consequence) = remove_words(name, has_file);
    for (line, mut text) in &mut lines {
        let words = match line {
            RemoveLine::Question => question.as_str(),
            RemoveLine::Consequence => consequence,
        };
        if text.0 != words {
            words.clone_into(&mut text.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(unsaved: bool, refusal: Option<(&str, Option<&str>)>) -> EmitterRow {
        EmitterRow {
            id: "1".into(),
            name: "Sparks".into(),
            unsaved,
            refusal: refusal.map(|(code, field)| (code.to_owned(), field.map(str::to_owned))),
            chosen: false,
        }
    }

    #[test]
    fn a_row_says_in_words_what_is_not_as_its_file_is() {
        assert_eq!(row_state(&row(false, None)), (String::new(), false));
        assert_eq!(row_state(&row(true, None)), ("Unsaved".to_owned(), false));
    }

    #[test]
    fn a_file_that_cannot_be_read_says_why_as_a_warning_and_never_as_a_code() {
        let (reason, warning) = row_state(&row(false, Some(("particle.decode", None))));
        assert!(warning);
        assert_eq!(reason, "The file is not an emitter this editor can read");
        // Each of the reader's refusals has words of its own.
        let (named, _) = row_state(&row(false, Some(("particle.value", Some("id")))));
        assert_eq!(named, "The file's name is not the ID of the emitter in it");
        let (value, _) = row_state(&row(
            false,
            Some(("particle.value", Some("lifetime.max_seconds"))),
        ));
        assert!(value.contains("longest life"), "{value}");
        for code in ["particle.limit", "particle.schema", "particle.read_failed"] {
            let (reason, warning) = row_state(&row(false, Some((code, None))));
            assert!(
                warning && !reason.is_empty() && !reason.contains("particle."),
                "{reason}"
            );
            assert_ne!(reason, named);
        }
    }

    #[test]
    fn the_question_names_the_emitter_and_says_what_removing_it_takes() {
        let (question, consequence) = remove_words("Sparks", true);
        assert_eq!(question, "Remove the emitter \u{201c}Sparks\u{201d}?");
        assert!(
            consequence.contains("file is removed") && consequence.contains("cannot be undone")
        );
        let (_, never_saved) = remove_words("Sparks", false);
        assert!(never_saved.contains("never saved") && never_saved.contains("cannot be undone"));
    }
}
