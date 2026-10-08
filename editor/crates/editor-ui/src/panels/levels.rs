//! The level dialog: open a level, save the level under another name, begin a new level.
//!
//! Levels are files in one folder, the one the open level is in, so the dialog lists that
//! folder and asks for a name; it does not browse the disk. Opening and saving go through the
//! core's jobs like every other open and save, and a failure shows in the notice.
//!
//! Opening another level and beginning a new one leave the open level, so they ask to
//! [`Leave`]: with unsaved changes the editor first asks whether to save them.

use std::path::{Path, PathBuf};

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{ButtonVariant, FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemedText},
    },
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::{EditableText, TextEdit},
    ui_widgets::Activate,
};

use crate::{
    panels::unsaved::Leave,
    session::Editor,
    theme::{
        feathers::{BORDER, SCRIM, SIGNAL_WARN},
        radius, spacing,
    },
};

/// What a level file's name ends with.
pub const LEVEL_SUFFIX: &str = ".level.json";

/// The longest name the dialog takes.
const NAME_LENGTH: usize = 48;

/// What the dialog is open for.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LevelDialog {
    #[default]
    Closed,
    /// Choose a level of the folder to open.
    Open,
    /// Name the file the level is saved to from now on.
    SaveAs,
    /// Name a new, empty level.
    New,
}

/// Why the name typed was not taken, shown under the field.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub struct LevelNameProblem(pub Option<&'static str>);

/// The dialog and the layer that covers the window behind it.
#[derive(Component, Default, Clone)]
pub struct LevelDialogRoot;

/// A text of the dialog that changes with what it is open for.
#[derive(Component, Clone, Copy, PartialEq, Eq, FromTemplate)]
pub enum LevelDialogText {
    #[default]
    Title,
    Hint,
    Problem,
    Confirm,
}

/// The list of the folder's levels.
#[derive(Component, Default, Clone)]
pub struct LevelList;

/// The button of one level in the list, and the file it opens.
#[derive(Component, Default, Clone)]
pub struct LevelRow(pub String);

/// The part of the dialog that asks for a name: the field, what is wrong with the name, and
/// the button that takes it.
#[derive(Component, Default, Clone)]
pub struct LevelNamePart;

/// The field the name is typed into.
#[derive(Component, Default, Clone)]
pub struct LevelNameInput;

/// The name was confirmed, by the button or by Enter.
#[derive(Event)]
pub struct ConfirmLevelName;

pub fn level_dialog() -> impl Scene {
    bsn! {
        LevelDialogRoot
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
        GlobalZIndex(100)
        ThemeBackgroundColor(SCRIM)
        Children [(
            pane_body()
            Node {
                width: px(420),
                max_width: percent(90),
                max_height: percent(90),
                row_gap: px(spacing::SPACE_4),
                padding: UiRect::all(px(spacing::SPACE_6)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(radius::RADIUS_FIELD)),
            }
            ThemeBorderColor(BORDER)
            Children [
                (label("") LevelDialogText::Title),
                (label_dim("") LevelDialogText::Hint),
                (
                    LevelList
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(spacing::SPACE_2),
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                    }
                ),
                (
                    LevelNamePart
                    Node { flex_direction: FlexDirection::Column, row_gap: px(spacing::SPACE_2) }
                    Children [
                        (
                            @FeathersTextInputContainer
                            Node { border: UiRect::all(px(1)), padding: UiRect::horizontal(px(spacing::SPACE_2)) }
                            ThemeBorderColor(BORDER)
                            Children [(
                                @FeathersTextInput { @max_characters: NAME_LENGTH }
                                LevelNameInput
                            )]
                        ),
                        (label("") LevelDialogText::Problem ThemeTextColor(SIGNAL_WARN)),
                    ]
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::End,
                        column_gap: px(spacing::SPACE_4),
                    }
                    Children [
                        (
                            @FeathersButton { @caption: bsn! { Text("Cancel") ThemedText } }
                            on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| {
                                *dialog = LevelDialog::Closed;
                            })
                        ),
                        (
                            @FeathersButton {
                                @caption: bsn! { Text("") ThemedText LevelDialogText::Confirm },
                                @variant: ButtonVariant::Primary,
                            }
                            LevelNamePart
                            on(|_: On<Activate>, mut commands: Commands| {
                                commands.trigger(ConfirmLevelName);
                            })
                        ),
                    ]
                ),
            ]
        )]
    }
}

/// The folder the levels are in: the open level's.
pub fn level_folder(level_path: &Path) -> PathBuf {
    level_path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// The names of the levels in `folder`, in order, without what a level file's name ends with.
/// A folder that is not there or cannot be read has no levels.
pub fn level_names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(LEVEL_SUFFIX).map(str::to_owned))
        .filter(|name| !name.is_empty())
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// The name of the level in `level_path`, as the dialog shows and takes it.
pub fn level_name(level_path: &Path) -> String {
    let file = level_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    file.strip_suffix(LEVEL_SUFFIX).unwrap_or(&file).to_owned()
}

/// The file of the level named `typed` in `folder`, or what is wrong with the name. `taken`
/// are the names the folder has; `own` is the open level's name if it has a file, which may
/// be given again.
pub fn level_file(
    folder: &Path,
    typed: &str,
    taken: &[String],
    own: Option<&str>,
) -> Result<PathBuf, &'static str> {
    let name = typed.trim();
    if name.is_empty() {
        return Err("Give the level a name.");
    }
    if name.starts_with('.') || name.contains(['/', '\\', ':']) || name.contains("..") {
        return Err("A name cannot begin with a full stop or contain / \\ : or two full stops.");
    }
    let same = |other: &String| other.to_lowercase() == name.to_lowercase();
    if taken.iter().any(same) && own.is_none_or(|own| own != name) {
        return Err("A level with that name exists. Choose another name.");
    }
    Ok(folder.join(format!("{name}{LEVEL_SUFFIX}")))
}

fn row(name: &str) -> impl Scene + use<> {
    let name = name.to_owned();
    let caption = name.clone();
    bsn! {
        @FeathersButton { @caption: bsn! { Text({caption}) ThemedText } }
        LevelRow({name})
        on(|activate: On<Activate>,
            mut commands: Commands,
            rows: Query<&LevelRow>,
            editor: NonSend<Editor>,
            mut dialog: ResMut<LevelDialog>| {
            if let Ok(row) = rows.get(activate.entity) {
                let file = level_folder(&editor.level_path).join(format!("{}{LEVEL_SUFFIX}", row.0));
                commands.trigger(Leave::Open(file));
                *dialog = LevelDialog::Closed;
            }
        })
    }
}

/// Shows the dialog for what it is open for, and hides it when it is closed.
#[expect(
    clippy::too_many_arguments,
    reason = "one system fills the whole dialog"
)]
pub(crate) fn show_level_dialog(
    mut commands: Commands,
    dialog: Res<LevelDialog>,
    problem: Res<LevelNameProblem>,
    editor: NonSend<Editor>,
    mut root: Query<&mut Node, (With<LevelDialogRoot>, Without<LevelNamePart>)>,
    mut name_parts: Query<&mut Node, (With<LevelNamePart>, Without<LevelDialogRoot>)>,
    list: Query<Entity, With<LevelList>>,
    mut texts: Query<(&LevelDialogText, &mut Text)>,
    mut input: Query<(Entity, &mut EditableText), With<LevelNameInput>>,
    mut focus: ResMut<InputFocus>,
) {
    if !dialog.is_changed() && !problem.is_changed() {
        return;
    }
    let display = |visible: bool| {
        if visible {
            Display::Flex
        } else {
            Display::None
        }
    };
    let asks_name = matches!(*dialog, LevelDialog::SaveAs | LevelDialog::New);
    for (which, mut text) in &mut texts {
        if *which == LevelDialogText::Problem {
            text.0 = problem.0.unwrap_or_default().to_owned();
        }
    }
    if !dialog.is_changed() {
        return;
    }
    for mut node in &mut root {
        node.display = display(*dialog != LevelDialog::Closed);
    }
    for mut node in &mut name_parts {
        node.display = display(asks_name);
    }
    let folder = level_folder(&editor.level_path);
    let names = if *dialog == LevelDialog::Open {
        level_names(&folder)
    } else {
        Vec::new()
    };
    let (title, hint, confirm) = match *dialog {
        LevelDialog::Closed => (String::new(), String::new(), ""),
        LevelDialog::Open => (
            "Open a level".to_owned(),
            if names.is_empty() {
                format!("There are no levels in {} yet.", folder.display())
            } else {
                format!("Levels in {}", folder.display())
            },
            "",
        ),
        LevelDialog::SaveAs => (
            "Save the level as".to_owned(),
            format!("It is saved in {} from now on.", folder.display()),
            "Save",
        ),
        LevelDialog::New => (
            "New level".to_owned(),
            format!("An empty level, saved in {}.", folder.display()),
            "Create",
        ),
    };
    for (which, mut text) in &mut texts {
        match which {
            LevelDialogText::Title => text.0.clone_from(&title),
            LevelDialogText::Hint => text.0.clone_from(&hint),
            LevelDialogText::Confirm => confirm.clone_into(&mut text.0),
            LevelDialogText::Problem => {}
        }
    }
    if let Ok(list) = list.single() {
        let rows: Vec<_> = names.iter().map(|name| row(name)).collect();
        commands
            .entity(list)
            .despawn_children()
            .queue_spawn_related_scenes::<Children>(rows);
    }
    if let Ok((entity, mut text)) = input.single_mut() {
        if asks_name {
            // Save As begins with the level's own name, a new level with none; the field has
            // the keyboard at once.
            let start = if *dialog == LevelDialog::SaveAs {
                level_name(&editor.level_path)
            } else {
                String::new()
            };
            text.queue_edit(TextEdit::SelectAll);
            text.queue_edit(TextEdit::Insert(start.as_str().into()));
            text.queue_edit(TextEdit::SelectAll);
            focus.set(entity, FocusCause::Navigated);
        } else if focus.get() == Some(entity) {
            focus.clear();
        }
    }
}

/// Opens the dialog with nothing left over from the last time.
pub(crate) fn forget_name_problem(dialog: Res<LevelDialog>, mut problem: ResMut<LevelNameProblem>) {
    if dialog.is_changed() && problem.0.is_some() {
        problem.0 = None;
    }
}

/// Escape closes the dialog; Enter in the name field confirms the name.
pub(crate) fn level_dialog_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut dialog: ResMut<LevelDialog>,
) {
    if *dialog == LevelDialog::Closed {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        *dialog = LevelDialog::Closed;
    } else if matches!(*dialog, LevelDialog::SaveAs | LevelDialog::New)
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter))
    {
        commands.trigger(ConfirmLevelName);
    }
}

/// Takes the name typed: saves the level there, or begins the new level there. A name that
/// cannot be taken leaves the dialog open and says why.
pub(crate) fn confirm_level_name(
    _: On<ConfirmLevelName>,
    mut commands: Commands,
    mut dialog: ResMut<LevelDialog>,
    mut problem: ResMut<LevelNameProblem>,
    mut editor: NonSendMut<Editor>,
    input: Query<&EditableText, With<LevelNameInput>>,
) {
    let Ok(text) = input.single() else {
        return;
    };
    let folder = level_folder(&editor.level_path);
    let own = (editor.has_file() && *dialog == LevelDialog::SaveAs)
        .then(|| level_name(&editor.level_path));
    let typed = text.value().to_string();
    match level_file(&folder, &typed, &level_names(&folder), own.as_deref()) {
        Ok(file) => {
            match *dialog {
                LevelDialog::SaveAs => editor.save_as(file),
                LevelDialog::New => commands.trigger(Leave::New(file)),
                LevelDialog::Open | LevelDialog::Closed => return,
            }
            *dialog = LevelDialog::Closed;
        }
        Err(why) => problem.0 = Some(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken() -> Vec<String> {
        vec!["Arena".to_owned(), "current-map".to_owned()]
    }

    #[test]
    fn a_name_becomes_a_level_file_in_the_folder() {
        let file = level_file(Path::new("levels"), "  Canyon 2 ", &taken(), None);
        assert_eq!(file, Ok(PathBuf::from("levels/Canyon 2.level.json")));
    }

    #[test]
    fn a_name_that_would_leave_the_folder_is_refused() {
        for name in ["", "   ", "../x", "a/b", "a\\b", ".hidden", "c:x", "a..b"] {
            assert!(
                level_file(Path::new("levels"), name, &[], None).is_err(),
                "{name:?}"
            );
        }
    }

    #[test]
    fn a_name_another_level_has_is_refused_whatever_its_case() {
        assert!(level_file(Path::new("levels"), "arena", &taken(), None).is_err());
        assert!(level_file(Path::new("levels"), "Arena", &taken(), Some("current-map")).is_err());
    }

    #[test]
    fn the_open_levels_own_name_may_be_given_again() {
        let file = level_file(Path::new("levels"), "Arena", &taken(), Some("Arena"));
        assert_eq!(file, Ok(PathBuf::from("levels/Arena.level.json")));
    }

    #[test]
    fn the_folder_lists_its_level_files_in_order_and_nothing_else() {
        let folder = tempfile::tempdir().unwrap();
        for file in [
            "b.level.json",
            "A.level.json",
            "notes.txt",
            ".level.json",
            "c.json",
        ] {
            std::fs::write(folder.path().join(file), "{}").unwrap();
        }
        std::fs::create_dir(folder.path().join("d.level.json")).unwrap();
        assert_eq!(level_names(folder.path()), ["A", "b"]);
        assert!(level_names(&folder.path().join("missing")).is_empty());
    }

    #[test]
    fn a_level_is_named_after_its_file() {
        assert_eq!(
            level_name(Path::new("levels/current-map.level.json")),
            "current-map"
        );
        assert_eq!(
            level_folder(Path::new("levels/x.level.json")),
            PathBuf::from("levels")
        );
        assert_eq!(level_folder(Path::new("x.level.json")), PathBuf::from("."));
    }
}
