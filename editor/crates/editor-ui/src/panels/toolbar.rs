//! The toolbar: one row at every window width.
//!
//! As the layout "Narrow window: one toolbar row with menus" has it: Level and Add are menus
//! at every width. In a window narrower than the design's fold width the tools fold into one
//! menu that names the chosen tool, and the two panel switches into another. A button or a row
//! is added when what it does exists.

use bevy::{
    feathers::{
        containers::pane_header_divider,
        controls::{
            ButtonVariant, FeathersButton, FeathersMenu, FeathersMenuButton, FeathersMenuDivider,
        },
        theme::{ThemeBackgroundColor, ThemedText},
        tokens,
    },
    gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSettings},
    prelude::*,
    ui_widgets::Activate,
    window::PrimaryWindow,
};

use crate::{
    panels::{
        levels::LevelDialog,
        menu::{menu_item, menu_popup, menu_popup_end},
        play::play_control,
    },
    session::Editor,
    shell::PanelChoice,
    theme::{size, spacing},
    tool::{PlaceAt, Tool},
};

/// A new landscape is two by two tiles of this many cells of this many metres each, 32 m on
/// a side in all. A starting size, not a limit: the level's size is not decided yet.
pub(crate) const LANDSCAPE_CELLS: u32 = 16;
pub(crate) const LANDSCAPE_SPACING: f32 = 1.0;

/// The toolbar's row.
#[derive(Component, Default, Clone)]
pub struct Toolbar;

/// A part of the toolbar that is there only at some window widths.
#[derive(Component, Clone, Copy, PartialEq, Eq, FromTemplate)]
pub enum Fold {
    /// Shown in a window wide enough for every tool and panel switch as a button.
    #[default]
    Wide,
    /// Shown in a narrower window: the menu the buttons fold into.
    Narrow,
}

/// Whether the tools and the panel switches fold into menus in a window this wide.
pub fn toolbar_folds(window_width: f32) -> bool {
    window_width < size::TOOLBAR_FOLD_WIDTH
}

/// What the tool buttons and the tool menu offer: the three ways the gizmo moves an object,
/// and the landscape's tools.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ToolChoice {
    #[default]
    Move,
    Rotate,
    Scale,
    Sculpt,
    Paint,
    Rules,
}

impl ToolChoice {
    pub const ALL: [ToolChoice; 6] = [
        Self::Move,
        Self::Rotate,
        Self::Scale,
        Self::Sculpt,
        Self::Paint,
        Self::Rules,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Sculpt => "Sculpt",
            Self::Paint => "Paint",
            Self::Rules => "Rules",
        }
    }

    /// The key that chooses it, as the tool menu shows it. Paint and Rules have none.
    fn keys(self) -> &'static str {
        match self {
            Self::Move => "W",
            Self::Rotate => "E",
            Self::Scale => "R",
            Self::Sculpt => "T",
            Self::Paint | Self::Rules => "",
        }
    }

    /// The choice that is in effect.
    pub fn current(tool: Tool, gizmo: TransformGizmoMode) -> Self {
        match (tool, gizmo) {
            (Tool::Sculpt, _) => Self::Sculpt,
            (Tool::Paint, _) => Self::Paint,
            (Tool::Rules, _) => Self::Rules,
            (Tool::Objects, TransformGizmoMode::Rotate) => Self::Rotate,
            (Tool::Objects, TransformGizmoMode::Scale) => Self::Scale,
            (Tool::Objects, _) => Self::Move,
        }
    }

    fn choose(self, tool: &mut Tool, gizmo: &mut TransformGizmoSettings) {
        let (wanted, mode) = match self {
            Self::Move => (Tool::Objects, Some(TransformGizmoMode::Translate)),
            Self::Rotate => (Tool::Objects, Some(TransformGizmoMode::Rotate)),
            Self::Scale => (Tool::Objects, Some(TransformGizmoMode::Scale)),
            Self::Sculpt => (Tool::Sculpt, None),
            Self::Paint => (Tool::Paint, None),
            Self::Rules => (Tool::Rules, None),
        };
        if let Some(mode) = mode {
            gizmo.mode = mode;
        }
        *tool = wanted;
    }
}

/// The button, or the menu row, of one tool.
#[derive(Component, Default, Clone)]
pub struct ToolSwitch(pub ToolChoice);

/// The caption of the tool menu's button, which names the chosen tool.
#[derive(Component, Default, Clone)]
pub struct ToolCaption;

fn choose_tool(
    activate: On<Activate>,
    switches: Query<&ToolSwitch>,
    mut gizmo: ResMut<TransformGizmoSettings>,
    mut tool: ResMut<Tool>,
) {
    if let Ok(switch) = switches.get(activate.entity) {
        switch.0.choose(&mut tool, &mut gizmo);
    }
}

fn tool_button(choice: ToolChoice) -> impl Scene {
    let name = choice.name();
    bsn! {
        @FeathersButton {
            @caption: bsn! { Text(name) ThemedText },
            @variant: ButtonVariant::Plain,
        }
        ToolSwitch({choice})
        on(choose_tool)
    }
}

fn tool_item(choice: ToolChoice) -> impl Scene {
    let (name, keys) = (choice.name(), choice.keys());
    bsn! {
        menu_item(name, keys)
        ToolSwitch({choice})
        on(choose_tool)
    }
}

fn level_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Children [
            (@FeathersMenuButton { @caption: bsn! { Text("Level") ThemedText } }),
            (
                menu_popup()
                Children [
                    (
                        menu_item("New level", "Ctrl+N")
                        on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| *dialog = LevelDialog::New)
                    ),
                    (
                        menu_item("Open level", "Ctrl+O")
                        on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| *dialog = LevelDialog::Open)
                    ),
                    @FeathersMenuDivider,
                    (
                        menu_item("Save", "Ctrl+S")
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.save())
                    ),
                    (
                        menu_item("Save as", "Ctrl+Shift+S")
                        on(|_: On<Activate>, mut dialog: ResMut<LevelDialog>| *dialog = LevelDialog::SaveAs)
                    ),
                    @FeathersMenuDivider,
                    (
                        menu_item("Cook", "")
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.cook())
                    ),
                ]
            ),
        ]
    }
}

fn add_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Children [
            (@FeathersMenuButton { @caption: bsn! { Text("Add") ThemedText } }),
            (
                menu_popup()
                Children [
                    (
                        menu_item("Landscape", "")
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| {
                            editor.add_landscape(LANDSCAPE_CELLS, LANDSCAPE_SPACING);
                        })
                    ),
                    (
                        menu_item("Box", "")
                        on(|_: On<Activate>, place: Res<PlaceAt>, mut editor: NonSendMut<Editor>| {
                            editor.add_box(&place);
                        })
                    ),
                    (
                        menu_item("Spawn point", "")
                        on(|_: On<Activate>, place: Res<PlaceAt>, mut editor: NonSendMut<Editor>| {
                            editor.add_spawn(&place);
                        })
                    ),
                ]
            ),
        ]
    }
}

/// The tools as one menu, for a narrow window. Its button names the chosen tool.
fn tool_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Fold::Narrow
        Node { display: Display::None }
        Children [
            (@FeathersMenuButton { @caption: bsn! { Text("Tool: Move") ThemedText ToolCaption } }),
            (
                menu_popup()
                Children [
                    tool_item(ToolChoice::Move),
                    tool_item(ToolChoice::Rotate),
                    tool_item(ToolChoice::Scale),
                    @FeathersMenuDivider,
                    tool_item(ToolChoice::Sculpt),
                    tool_item(ToolChoice::Paint),
                    tool_item(ToolChoice::Rules),
                ]
            ),
        ]
    }
}

/// The two panel switches as one menu, for a narrow window.
fn panels_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Fold::Narrow
        Node { display: Display::None }
        Children [
            (@FeathersMenuButton { @caption: bsn! { Text("Panels") ThemedText } }),
            (
                menu_popup_end()
                Children [
                    (
                        menu_item("Hierarchy", "")
                        on(|_: On<Activate>, mut choice: ResMut<PanelChoice>| choice.toggle_hierarchy())
                    ),
                    (
                        menu_item("Inspector", "")
                        on(|_: On<Activate>, mut choice: ResMut<PanelChoice>| choice.toggle_inspector())
                    ),
                ]
            ),
        ]
    }
}

fn group() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_2),
            flex_shrink: 0.0,
        }
    }
}

pub fn toolbar() -> impl Scene {
    bsn! {
        Toolbar
        Node {
            height: px(size::TOOLBAR_HEIGHT),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_2),
            padding: UiRect::axes(px(spacing::SPACE_4), px(spacing::SPACE_2)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [
            level_menu(),
            pane_header_divider(),
            (
                @FeathersButton {
                    @caption: bsn! { Text("Undo") ThemedText },
                    @variant: ButtonVariant::Plain,
                }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.undo())
            ),
            (
                @FeathersButton {
                    @caption: bsn! { Text("Redo") ThemedText },
                    @variant: ButtonVariant::Plain,
                }
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.redo())
            ),
            pane_header_divider(),
            (
                group()
                Fold::Wide
                Children [
                    tool_button(ToolChoice::Move),
                    tool_button(ToolChoice::Rotate),
                    tool_button(ToolChoice::Scale),
                    tool_button(ToolChoice::Sculpt),
                    tool_button(ToolChoice::Paint),
                    tool_button(ToolChoice::Rules),
                ]
            ),
            tool_menu(),
            add_menu(),
            (
                // In a narrow window the Delete key removes; the row has no room for the
                // button beside the Play control, and the design's narrow toolbar has none.
                @FeathersButton {
                    @caption: bsn! { Text("Remove") ThemedText },
                    @variant: ButtonVariant::Plain,
                }
                Fold::Wide
                on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.delete_selected())
            ),
            Node { flex_grow: 1.0, min_width: px(spacing::SPACE_4) },
            // The Play control is at the right end at every width and never folds away.
            play_control(),
            pane_header_divider(),
            (
                group()
                Fold::Wide
                Children [
                    (
                        @FeathersButton { @caption: bsn! { Text("Hierarchy") ThemedText } }
                        on(|_: On<Activate>, mut choice: ResMut<PanelChoice>| choice.toggle_hierarchy())
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Inspector") ThemedText } }
                        on(|_: On<Activate>, mut choice: ResMut<PanelChoice>| choice.toggle_inspector())
                    ),
                ]
            ),
            panels_menu(),
        ]
    }
}

/// Shows the buttons or the menus they fold into, by the window's width.
pub(crate) fn fold_toolbar(
    window: Single<&Window, With<PrimaryWindow>>,
    mut parts: Query<(&Fold, &mut Node)>,
) {
    let folded = toolbar_folds(window.width());
    for (part, mut node) in &mut parts {
        let wanted = if (*part == Fold::Narrow) == folded {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// Shows which tool is chosen: its button is the primary one, and the tool menu names it.
pub(crate) fn show_tool(
    tool: Res<Tool>,
    gizmo: Res<TransformGizmoSettings>,
    mut buttons: Query<(&ToolSwitch, &mut ButtonVariant)>,
    mut captions: Query<&mut Text, With<ToolCaption>>,
) {
    // Compared each frame and written only on a change: the tool is set from several places,
    // and the buttons may be made after it was.
    let current = ToolChoice::current(*tool, gizmo.mode);
    for (switch, mut variant) in &mut buttons {
        let wanted = if switch.0 == current {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Plain
        };
        if *variant != wanted {
            *variant = wanted;
        }
    }
    let caption = tool_caption(current);
    for mut text in &mut captions {
        if text.0 != caption {
            text.0.clone_from(&caption);
        }
    }
}

/// What the tool menu's button says.
pub fn tool_caption(choice: ToolChoice) -> String {
    format!("Tool: {}", choice.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_fold_below_the_designs_width_and_not_at_it() {
        assert!(toolbar_folds(640.0));
        assert!(toolbar_folds(size::TOOLBAR_FOLD_WIDTH - 1.0));
        assert!(!toolbar_folds(size::TOOLBAR_FOLD_WIDTH));
        assert!(!toolbar_folds(1440.0));
    }

    #[test]
    fn choosing_a_tool_and_reading_the_choice_back_agree_for_every_tool() {
        for choice in ToolChoice::ALL {
            // From the state farthest from the choice: another tool and another gizmo mode.
            let mut tool = if choice == ToolChoice::Rules {
                Tool::Sculpt
            } else {
                Tool::Rules
            };
            let mut gizmo = TransformGizmoSettings {
                mode: if choice == ToolChoice::Scale {
                    TransformGizmoMode::Rotate
                } else {
                    TransformGizmoMode::Scale
                },
                ..default()
            };
            choice.choose(&mut tool, &mut gizmo);
            assert_eq!(ToolChoice::current(tool, gizmo.mode), choice, "{choice:?}");
        }
    }

    #[test]
    fn a_landscape_tool_keeps_the_gizmo_mode_for_the_way_back() {
        let mut tool = Tool::Objects;
        let mut gizmo = TransformGizmoSettings {
            mode: TransformGizmoMode::Rotate,
            ..default()
        };
        ToolChoice::Sculpt.choose(&mut tool, &mut gizmo);
        assert_eq!(tool, Tool::Sculpt);
        assert_eq!(gizmo.mode, TransformGizmoMode::Rotate);
    }
}
