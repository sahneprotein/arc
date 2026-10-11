//! The toolbar: one row at every window width.
//!
//! The tools are the ways the gizmo moves an object, the landscape's tools, Review, which
//! lists in the right-hand panel what the core finds wrong with the level, and Particles,
//! which shows the game's particle emitters in the level's place. While Particles is chosen
//! Undo, Redo and Remove act on the chosen emitter, not on the level.
//!
//! As the layout "Narrow window: one toolbar row with menus" has it: Level and Add are menus
//! at every width. In a window narrower than the design's fold width the tools fold into one
//! menu that names the chosen tool, the snap switch into a row of that menu, and the two panel
//! switches into another. The tools also fold into their menu in a wider window whose row
//! cannot hold them as buttons, which is measured: the row keeps to one line at every width.
//! A button or a row is added when what it does exists.

use bevy::{
    app::PropagateOver,
    feathers::{
        constants::fonts,
        containers::pane_header_divider,
        controls::{
            ButtonVariant, FeathersButton, FeathersMenu, FeathersMenuButton, FeathersMenuDivider,
        },
        theme::{ThemeBackgroundColor, ThemeTextColor, ThemedText},
        tokens,
    },
    gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSettings},
    prelude::*,
    text::FontSourceTemplate,
    ui::{ComputedNode, InteractionDisabled, UiGlobalTransform},
    ui_widgets::Activate,
    window::PrimaryWindow,
};

use crate::{
    panels::{
        levels::LevelDialog,
        menu::{menu_item, menu_popup, menu_popup_end},
        play::play_control,
        project::ProjectDialog,
        snap::{snap_button, snap_item},
    },
    particles::Particles,
    session::{duplicate_offer, emitter_placements, DuplicateOffer, Editor},
    shell::PanelChoice,
    theme::{size, spacing, text},
    tool::{PlaceAt, Tool},
};

/// A new landscape is two by two tiles of this many cells of this many metres each, 32 m on
/// a side in all. A starting size, not a limit: the level's size is not decided yet.
pub(crate) const LANDSCAPE_CELLS: u32 = 16;
pub(crate) const LANDSCAPE_SPACING: f32 = 1.0;

/// The toolbar's row.
#[derive(Component, Default, Clone)]
pub struct Toolbar;

/// Takes the last change back: the chosen emitter's while the Particles tool is chosen, the
/// level's otherwise. Each has a history of its own, and the one on screen is the one meant.
pub fn undo(tool: Tool, particles: &mut Particles, editor: &mut Editor) {
    if tool == Tool::Particles {
        particles.undo();
    } else {
        editor.undo();
    }
}

/// Makes the change again that was last taken back, of the emitter or of the level as
/// [`undo`] has it.
pub fn redo(tool: Tool, particles: &mut Particles, editor: &mut Editor) {
    if tool == Tool::Particles {
        particles.redo();
    } else {
        editor.redo();
    }
}

/// Removes what is chosen: the selected objects of the level, or, while the Particles tool
/// is chosen, the chosen emitter, which is asked about first. An emitter that the open level
/// places is not removed: the panel says so.
pub fn remove(tool: Tool, particles: &mut Particles, editor: &mut Editor) {
    if tool == Tool::Particles {
        let placements = particles
            .chosen_id()
            .map_or(0, |emitter| emitter_placements(&editor.snapshot(), emitter));
        particles.ask_remove(placements);
    } else {
        editor.delete_selected();
    }
}

/// The Add menu adds to the level, which the Particles tool does not show: what is added
/// would not be seen. Adding from there goes back to the object tools first.
fn back_to_the_level(tool: &mut Tool) {
    if *tool == Tool::Particles {
        *tool = Tool::Objects;
    }
}

/// A part of the toolbar that is there only at some window widths.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, FromTemplate)]
pub enum Fold {
    /// Shown in a window wide enough for the panel switches as buttons.
    #[default]
    Wide,
    /// Shown in a narrower window: the menu those buttons fold into.
    Narrow,
    /// The tools as buttons, and the snap switch beside them: shown while the row holds
    /// them.
    WideTools,
    /// The menu the tools fold into: shown in a narrow window, and in a wider one whose row
    /// cannot hold the tools as buttons.
    NarrowTools,
}

impl Fold {
    /// Whether the part is laid out. `folded` says that the window is narrower than the
    /// design's fold width, and `tools_folded` that the tools are in their menu.
    pub fn shown(self, folded: bool, tools_folded: bool) -> bool {
        match self {
            Self::Wide => !folded,
            Self::Narrow => folded,
            Self::WideTools => !tools_folded,
            Self::NarrowTools => tools_folded,
        }
    }
}

/// What takes up the room the toolbar's row has left, between the tools and the Play control.
#[derive(Component, Default, Clone)]
pub struct ToolbarSpacer;

/// Whether the tools are in their menu although the window is wider than the fold width.
///
/// The row is as long as its captions make it, and the Play control grows by a word and Stop
/// while a game runs, so the width at which the tools fit as buttons is measured and not
/// written down: the tools fold into their menu when the row is longer than the window, and
/// come back when the row with them as buttons would fit again.
#[derive(Default)]
pub(crate) struct ToolsFold {
    folded: bool,
    /// Frames since what is laid out last changed here: a row is measured only once it has
    /// been laid out as it is.
    settled: u8,
    /// How wide the tools are as buttons, with the gap between them, when last seen so.
    wide_width: f32,
}

/// Frames a change of the row needs to be laid out before the row is measured again.
const FRAMES_TO_SETTLE: u8 = 2;

/// Half a physical pixel of slack for rounding, in logical pixels.
const FOLD_SLACK: f32 = 0.5;

/// Whether the tools are to be in their menu. `needed` is the width the row needs as it is
/// laid out now, `narrow` the width of the tool menu's button in it (nothing while the tools
/// are buttons), `wide` the width of the tools as buttons, and `available` the window's.
pub fn tools_fold(folded: bool, needed: f32, narrow: f32, wide: f32, available: f32) -> bool {
    if folded {
        // They come back when the row with them as buttons would fit.
        needed - narrow + wide > available - FOLD_SLACK
    } else {
        needed > available + FOLD_SLACK
    }
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
    Review,
    Particles,
}

impl ToolChoice {
    pub const ALL: [ToolChoice; 8] = [
        Self::Move,
        Self::Rotate,
        Self::Scale,
        Self::Sculpt,
        Self::Paint,
        Self::Rules,
        Self::Review,
        Self::Particles,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Sculpt => "Sculpt",
            Self::Paint => "Paint",
            Self::Rules => "Rules",
            Self::Review => "Review",
            Self::Particles => "Particles",
        }
    }

    /// The key that chooses it, as the tool menu shows it. Paint and Rules have none.
    fn keys(self) -> &'static str {
        match self {
            Self::Move => "W",
            Self::Rotate => "E",
            Self::Scale => "R",
            Self::Sculpt => "T",
            Self::Paint | Self::Rules | Self::Review | Self::Particles => "",
        }
    }

    /// The choice that is in effect.
    pub fn current(tool: Tool, gizmo: TransformGizmoMode) -> Self {
        match (tool, gizmo) {
            (Tool::Sculpt, _) => Self::Sculpt,
            (Tool::Paint, _) => Self::Paint,
            (Tool::Rules, _) => Self::Rules,
            (Tool::Review, _) => Self::Review,
            (Tool::Particles, _) => Self::Particles,
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
            Self::Review => (Tool::Review, None),
            Self::Particles => (Tool::Particles, None),
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
                    @FeathersMenuDivider,
                    (
                        // The project's settings are to be had whatever is open.
                        menu_item("Project settings", "")
                        on(|_: On<Activate>, mut dialog: ResMut<ProjectDialog>| dialog.open = true)
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
                        on(|_: On<Activate>, mut tool: ResMut<Tool>, mut editor: NonSendMut<Editor>| {
                            back_to_the_level(&mut tool);
                            editor.add_landscape(LANDSCAPE_CELLS, LANDSCAPE_SPACING);
                        })
                    ),
                    (
                        menu_item("Box", "")
                        on(|_: On<Activate>, place: Res<PlaceAt>, mut tool: ResMut<Tool>, mut editor: NonSendMut<Editor>| {
                            back_to_the_level(&mut tool);
                            editor.add_box(&place);
                        })
                    ),
                    (
                        menu_item("Spawn point", "")
                        on(|_: On<Activate>, place: Res<PlaceAt>, mut tool: ResMut<Tool>, mut editor: NonSendMut<Editor>| {
                            back_to_the_level(&mut tool);
                            editor.add_spawn(&place);
                        })
                    ),
                    @FeathersMenuDivider,
                    (
                        menu_item("Duplicate", "Ctrl+D")
                        DuplicateRow
                        InteractionDisabled
                        on(|_: On<Activate>, mut tool: ResMut<Tool>, mut editor: NonSendMut<Editor>| {
                            back_to_the_level(&mut tool);
                            editor.duplicate_selected();
                        })
                    ),
                    (
                        // Why the row above is off, beside it, as the design's Menu has it.
                        Text("")
                        TextFont {
                            font: FontSourceTemplate::Handle(fonts::REGULAR),
                            font_size: {FontSize::Px(text::SMALL.size)},
                        }
                        PropagateOver<TextFont>
                        ThemeTextColor(tokens::TEXT_DIM)
                        DuplicateReason
                        Node {
                            max_width: px(size::MENU_WIDTH),
                            padding: UiRect::axes(px(spacing::SPACE_4), px(spacing::SPACE_1)),
                        }
                    ),
                ]
            ),
        ]
    }
}

/// The Add menu's row that copies the selection.
#[derive(Component, Default, Clone)]
pub struct DuplicateRow;

/// The line under it that says why it is off.
#[derive(Component, Default, Clone)]
pub struct DuplicateReason;

/// Why "Duplicate" cannot be chosen. Empty when it can.
pub fn duplicate_reason(offer: DuplicateOffer) -> &'static str {
    match offer {
        DuplicateOffer::Offered => "",
        DuplicateOffer::NothingSelected => "Select what to duplicate.",
        DuplicateOffer::Landscape => {
            "The selection includes a landscape. Change the selection before duplicating."
        }
        DuplicateOffer::NoNumberLeft => {
            "The level has no number left for a copy. Remove an object first."
        }
    }
}

/// Switches "Duplicate" on or off with the selection, and says why it is off. Worked out
/// when the selection or the hierarchy changed, and when the row is new.
pub(crate) fn show_duplicate(
    mut commands: Commands,
    editor: NonSend<Editor>,
    rows: Query<(Entity, Has<InteractionDisabled>), With<DuplicateRow>>,
    added: Query<(), Added<DuplicateRow>>,
    mut reasons: Query<(&mut Text, &mut Node), With<DuplicateReason>>,
    mut shown: Local<Option<(u64, u64, u64)>>,
) {
    let snapshot = editor.snapshot();
    let key = (
        snapshot.session,
        snapshot.regions.selection,
        snapshot.regions.hierarchy,
    );
    if *shown == Some(key) && added.is_empty() {
        return;
    }
    *shown = Some(key);
    let offer = duplicate_offer(&snapshot);
    let offered = offer == DuplicateOffer::Offered;
    for (row, off) in &rows {
        if offered && off {
            commands.entity(row).remove::<InteractionDisabled>();
        } else if !offered && !off {
            commands.entity(row).insert(InteractionDisabled);
        }
    }
    let reason = duplicate_reason(offer);
    let display = if offered {
        Display::None
    } else {
        Display::Flex
    };
    for (mut text, mut node) in &mut reasons {
        if text.0 != reason {
            text.0 = reason.to_owned();
        }
        if node.display != display {
            node.display = display;
        }
    }
}

/// The tools as one menu, for a narrow window. Its button names the chosen tool.
fn tool_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Fold::NarrowTools
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
                    @FeathersMenuDivider,
                    tool_item(ToolChoice::Review),
                    tool_item(ToolChoice::Particles),
                    @FeathersMenuDivider,
                    snap_item(),
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
                on(|_: On<Activate>, tool: Res<Tool>, mut particles: ResMut<Particles>, mut editor: NonSendMut<Editor>| {
                    undo(*tool, &mut particles, &mut editor);
                })
            ),
            (
                @FeathersButton {
                    @caption: bsn! { Text("Redo") ThemedText },
                    @variant: ButtonVariant::Plain,
                }
                on(|_: On<Activate>, tool: Res<Tool>, mut particles: ResMut<Particles>, mut editor: NonSendMut<Editor>| {
                    redo(*tool, &mut particles, &mut editor);
                })
            ),
            pane_header_divider(),
            (
                group()
                Fold::WideTools
                Children [
                    tool_button(ToolChoice::Move),
                    tool_button(ToolChoice::Rotate),
                    tool_button(ToolChoice::Scale),
                    tool_button(ToolChoice::Sculpt),
                    tool_button(ToolChoice::Paint),
                    tool_button(ToolChoice::Rules),
                    tool_button(ToolChoice::Review),
                    tool_button(ToolChoice::Particles),
                ]
            ),
            // Snap is a toggle of its own beside the tools, not one of them.
            (snap_button() Fold::WideTools),
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
                on(|_: On<Activate>, tool: Res<Tool>, mut particles: ResMut<Particles>, mut editor: NonSendMut<Editor>| {
                    remove(*tool, &mut particles, &mut editor);
                })
            ),
            (Node { flex_grow: 1.0, min_width: px(spacing::SPACE_4) } ToolbarSpacer),
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

/// Shows the buttons or the menus they fold into: by the window's width against the design's
/// fold width, and for the tools also by whether the row can hold them ([`ToolsFold`]).
pub(crate) fn fold_toolbar(
    window: Single<&Window, With<PrimaryWindow>>,
    bar: Query<(&ComputedNode, &UiGlobalTransform, &Children), With<Toolbar>>,
    placed: Query<(&ComputedNode, &UiGlobalTransform, Has<ToolbarSpacer>)>,
    mut parts: Query<(&Fold, &mut Node, &ComputedNode)>,
    mut tools: Local<ToolsFold>,
) {
    let folded = toolbar_folds(window.width());
    tools.settled = tools.settled.saturating_add(1);
    // In a narrow window everything is folded and there is nothing to measure. Otherwise the
    // row is measured as it was last laid out: how long it is, with the room it has to spare
    // taken off and what hangs over its end added.
    if !folded
        && tools.settled >= FRAMES_TO_SETTLE
        && let Ok((bar_node, bar_at, children)) = bar.single()
        && !bar_node.is_empty()
    {
        let scale = bar_node.inverse_scale_factor;
        let end = (bar_at.translation.x + bar_node.size.x / 2.0) * scale - spacing::SPACE_4;
        let (mut last, mut spare) = (f32::MIN, 0.0_f32);
        for child in children.iter() {
            let Ok((node, at, spacer)) = placed.get(child) else {
                continue;
            };
            if node.is_empty() {
                continue;
            }
            last = last.max((at.translation.x + node.size.x / 2.0) * scale);
            if spacer {
                spare = (node.size.x * scale - spacing::SPACE_4).max(0.0);
            }
        }
        let needed = bar_node.size.x * scale - spare + (last - end).max(0.0);
        let (mut wide, mut narrow) = (0.0_f32, 0.0_f32);
        for (part, _, node) in &parts {
            let width = node.size.x * node.inverse_scale_factor;
            match part {
                Fold::WideTools if width > 0.0 => wide += width + spacing::SPACE_2,
                Fold::NarrowTools => narrow += width,
                _ => {}
            }
        }
        if !tools.folded {
            // Two parts with one gap between them; the gap before them is there either way.
            tools.wide_width = (wide - spacing::SPACE_2).max(0.0);
        }
        let wanted = tools_fold(
            tools.folded,
            needed,
            narrow,
            tools.wide_width,
            window.width(),
        );
        if wanted != tools.folded {
            tools.folded = wanted;
            tools.settled = 0;
        }
    }
    let tools_folded = folded || tools.folded;
    for (part, mut node, _) in &mut parts {
        let wanted = if part.shown(folded, tools_folded) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted {
            node.display = wanted;
            tools.settled = 0;
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
    fn a_row_that_is_off_says_why_and_one_that_is_on_says_nothing() {
        assert_eq!(duplicate_reason(DuplicateOffer::Offered), "");
        assert!(duplicate_reason(DuplicateOffer::NothingSelected).contains("Select"));
        assert!(duplicate_reason(DuplicateOffer::Landscape).contains("landscape"));
        assert!(duplicate_reason(DuplicateOffer::NoNumberLeft).contains("no number left"));
    }

    #[test]
    fn the_tools_fold_when_the_row_is_longer_than_the_window_and_come_back_when_they_fit() {
        // The row needs 1060 with the tools as buttons, which take 470 of it; their menu's
        // button takes 90.
        let (wide, narrow) = (470.0, 90.0);
        assert!(tools_fold(false, 1060.0, 0.0, wide, 900.0));
        assert!(tools_fold(false, 1060.0, 0.0, wide, 1059.0));
        assert!(!tools_fold(false, 1060.0, 0.0, wide, 1060.0));
        // Folded, the row needs 680. They stay folded until 1060 would fit again.
        let folded_needs = 1060.0 - wide + narrow;
        assert!(tools_fold(true, folded_needs, narrow, wide, 1000.0));
        assert!(tools_fold(true, folded_needs, narrow, wide, 1060.0));
        assert!(!tools_fold(true, folded_needs, narrow, wide, 1061.0));
        // While a game runs the Play control is longer, and so is what is needed.
        assert!(tools_fold(true, folded_needs + 110.0, narrow, wide, 1100.0));
    }

    #[test]
    fn each_part_of_the_row_shows_with_its_fold() {
        // Wider than the fold width, the tools in their menu: the panel switches are buttons.
        assert!(Fold::Wide.shown(false, true) && !Fold::Narrow.shown(false, true));
        assert!(Fold::NarrowTools.shown(false, true) && !Fold::WideTools.shown(false, true));
        // Wide enough for everything.
        assert!(Fold::WideTools.shown(false, false) && !Fold::NarrowTools.shown(false, false));
        // Narrower than the fold width: every menu, no button.
        assert!(Fold::Narrow.shown(true, true) && Fold::NarrowTools.shown(true, true));
        assert!(!Fold::Wide.shown(true, true) && !Fold::WideTools.shown(true, true));
    }

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
