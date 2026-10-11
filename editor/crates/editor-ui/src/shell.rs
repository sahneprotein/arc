//! The editor window: toolbar, hierarchy, 3D view, inspector, status bar.
//!
//! The regions and their sizes are those of the layout "Editing: scene authoring". The side
//! panels give way on a narrow window so that the 3D view keeps its minimum width; a toolbar
//! button brings a panel back, or hides it, whatever the width.

use bevy::{
    feathers::FeathersPlugins,
    input_focus::tab_navigation::TabGroup,
    prelude::*,
    ui_widgets::{ControlOrientation, Scrollbar},
    window::{PrimaryWindow, RequestRedraw},
};

use crate::{
    panels::{
        brush::{brush_panel, show_brush_mode, show_landscape_parts, BrushPanel},
        closing::{drive_shutdown, shutdown_dialog, Shutdown},
        emitter::{
            commit_emitter_name, emitter_panel, fill_emitter_fields, fill_emitter_names,
            fit_emitter, preview_overlay, run_preview, show_emitter, show_preview,
        },
        emitters::{
            emitters_panel, fit_emitters, remove_dialog, show_emitters, show_remove_dialog,
        },
        empty::{empty_hint, show_empty_hint},
        hierarchy::{
            fold_rows, hierarchy_panel, show_drop_target, show_hierarchy, show_selection,
            Collapsed, HierarchyPanel, RowDrag,
        },
        inspector::{
            commit_name, fill_names, fill_new_inputs, fill_snap_steps, inspector_panel,
            show_inspector, show_mixed_marks, InspectorPanel,
        },
        levels::{
            confirm_level_name, forget_name_problem, level_dialog, level_dialog_keys,
            show_level_dialog, LevelDialog, LevelNameProblem,
        },
        menu::{keys_show_row_focus, mark_menu_open, menu_scrim, show_row_focus, MenuOpen},
        notice::{notice, show_notice},
        paint::{commit_layer_name, fill_layer_names, show_layers},
        play::show_play,
        review::{fit_review, review_panel, show_review},
        rules::{fill_rule_fields, show_rules},
        snap::{mirror_snap, show_snap, snap_overlay},
        status::{show_status, status_bar},
        toolbar::{fold_toolbar, show_duplicate, show_tool, toolbar},
        unsaved::{
            answer_unsaved, close_requested, leave_or_ask, unsaved_dialog, Question, Unsaved,
        },
        view::{show_view, view_control},
    },
    particles::{GpuPreview, Particles, Preview},
    session::{pump_editor, Editor, PumpEditor},
    theme::{feathers::ui_theme, size, spacing},
    tool::{Brush, BrushAim, ChosenRule, LookStatus, PaintLayer, PlaceAt, Snap, Tool, ViewMode},
};

/// The node the application puts the 3D view in.
#[derive(Component, Default, Clone)]
pub struct ViewportSlot;

/// Whether a dialog or a menu is open in front of the window. While one is, the 3D view and
/// the shortcuts do nothing: the dialog or the menu has the pointer and the keyboard.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Modal(pub bool);

fn mark_modal(
    unsaved: Res<Unsaved>,
    levels: Res<LevelDialog>,
    menu: Res<MenuOpen>,
    shutdown: Res<Shutdown>,
    particles: Res<Particles>,
    mut modal: ResMut<Modal>,
) {
    let open = unsaved.question == Question::Asking
        || *levels != LevelDialog::Closed
        || menu.0
        || *shutdown != Shutdown::No
        || particles.removing().is_some();
    if modal.0 != open {
        modal.0 = open;
    }
}

/// What the person chose for each side panel. `None` follows the window's width.
#[derive(Resource, Default)]
pub struct PanelChoice {
    hierarchy: Option<bool>,
    inspector: Option<bool>,
    /// What is shown now, so that a toggle flips what the person sees.
    shown: (bool, bool),
}

impl PanelChoice {
    pub fn toggle_hierarchy(&mut self) {
        self.hierarchy = Some(!self.shown.0);
    }

    pub fn toggle_inspector(&mut self) {
        self.inspector = Some(!self.shown.1);
    }

    /// Whether the hierarchy and the right-hand panel are shown now.
    pub fn shown(&self) -> (bool, bool) {
        self.shown
    }

    /// Shows or hides the side panels whatever the window's width. `None` follows the width.
    pub fn set(&mut self, hierarchy: Option<bool>, inspector: Option<bool>) {
        self.hierarchy = hierarchy;
        self.inspector = inspector;
    }
}

/// Which side panels fit beside a 3D view of its minimum width. The inspector gives way
/// first, then the hierarchy.
pub fn panels_that_fit(window_width: f32) -> (bool, bool) {
    let hierarchy = window_width >= size::HIERARCHY_WIDTH + size::VIEWPORT_MIN_WIDTH;
    let inspector =
        window_width >= size::HIERARCHY_WIDTH + size::INSPECTOR_WIDTH + size::VIEWPORT_MIN_WIDTH;
    (hierarchy, inspector)
}

fn shell() -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(spacing::SPACE_1),
        }
        // The two layout nodes cover the 3D view without drawing anything. They must not take
        // the pointer, or a click would never reach an object in the view.
        Pickable { should_block_lower: false, is_hoverable: false }
        TabGroup
        Children [
            toolbar(),
            (
                Node {
                    flex_grow: 1.0,
                    min_height: px(0),
                    flex_direction: FlexDirection::Row,
                    column_gap: px(spacing::SPACE_1),
                }
                Pickable { should_block_lower: false, is_hoverable: false }
                Children [
                    hierarchy_panel(),
                    emitters_panel(),
                    // Nothing is drawn here: the 3D view shows through. The node takes camera
                    // input and lets clicks through to the objects behind it.
                    (
                        Node { flex_grow: 1.0, min_width: px(0) }
                        Pickable { should_block_lower: false, is_hoverable: true }
                        ViewportSlot
                        Children [
                            empty_hint(),
                            notice(),
                            snap_overlay(),
                            view_control(),
                            preview_overlay(),
                        ]
                    ),
                    inspector_panel(),
                    brush_panel(),
                    review_panel(),
                    emitter_panel(),
                ]
            ),
            status_bar(),
            menu_scrim(),
            level_dialog(),
            unsaved_dialog(),
            remove_dialog(),
            shutdown_dialog(),
        ]
    }
}

type Only<A, B, C> = (With<A>, Without<B>, Without<C>);

/// Shows the side panels that fit and that the person chose. The right-hand place holds the
/// inspector, or the landscape tool's settings while that tool is chosen, or the Review panel
/// while Review is (`review::fit_review`). While the Particles tool is chosen the two places
/// hold the emitters' list and the chosen emitter's settings (`emitters::fit_emitters`,
/// `emitter::fit_emitter`): the level's hierarchy is not shown then.
fn fit_panels(
    window: Single<&Window, With<PrimaryWindow>>,
    tool: Res<Tool>,
    mut choice: ResMut<PanelChoice>,
    mut hierarchy: Query<&mut Node, Only<HierarchyPanel, InspectorPanel, BrushPanel>>,
    mut inspector: Query<&mut Node, Only<InspectorPanel, HierarchyPanel, BrushPanel>>,
    mut brush: Query<&mut Node, Only<BrushPanel, HierarchyPanel, InspectorPanel>>,
) {
    let fit = panels_that_fit(window.width());
    let show = (
        choice.hierarchy.unwrap_or(fit.0),
        choice.inspector.unwrap_or(fit.1),
    );
    if choice.shown != show {
        choice.shown = show;
    }
    let display = |shown: bool| if shown { Display::Flex } else { Display::None };
    for mut node in &mut hierarchy {
        let listing = show.0 && *tool != Tool::Particles;
        if node.display != display(listing) {
            node.display = display(listing);
        }
    }
    let sculpting = tool.is_landscape();
    for mut node in &mut inspector {
        let inspecting = show.1 && *tool == Tool::Objects;
        if node.display != display(inspecting) {
            node.display = display(inspecting);
        }
    }
    for mut node in &mut brush {
        if node.display != display(show.1 && sculpting) {
            node.display = display(show.1 && sculpting);
        }
    }
}

/// How many more frames are drawn after something changed. A change made in one frame is
/// often only on screen two frames later: a panel is told to rebuild, its new rows are made,
/// and only then laid out.
const FRAMES_AFTER_A_CHANGE: u8 = 4;

/// Keeps frames coming for a moment after anything changed.
///
/// The editor draws only when the desktop sends an input event or something asks for a frame.
/// A click arrives as two events, press and release; whatever that click sets going (a
/// command to the core, another tool's panel, a dialog) needs frames after the release to
/// appear, and nothing would ask for them: the window would show the old state until the
/// mouse moved. So every frame in which the pointer or the keyboard did something, the core
/// owed or delivered something, or one of the editor's own choices changed, asks for a few
/// more.
///
/// A game the editor started is not such a thing: while it runs nothing changes here, and the
/// editor sleeps. What the Play coordinator reports of it (a step, the end) counts as a change
/// like any other, once: the coordinator wakes the editor for it.
#[expect(
    clippy::type_complexity,
    reason = "every choice the panels show is looked at once"
)]
fn keep_drawing(
    editor: NonSend<Editor>,
    (mouse, keys): (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>),
    choices: (
        Res<Tool>,
        Res<Brush>,
        Res<PaintLayer>,
        Res<ChosenRule>,
        Res<PanelChoice>,
        Res<LevelDialog>,
        Res<LevelNameProblem>,
        Res<Unsaved>,
        Res<Collapsed>,
        Res<Modal>,
        Res<ViewMode>,
        Res<Particles>,
        Res<Preview>,
        Res<GpuPreview>,
    ),
    mut seen: Local<Option<(u64, u64, u64, bool)>>,
    mut frames_left: Local<u8>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let snapshot = editor.snapshot();
    let view = (
        snapshot.session,
        snapshot.view_revision,
        editor.play_status().map_or(0, |status| status.revision),
        editor.play_requested(),
    );
    let input = mouse.get_just_pressed().len() + mouse.get_just_released().len() > 0
        || keys.get_just_pressed().len() + keys.get_just_released().len() > 0;
    let (
        tool,
        brush,
        layer,
        rule,
        panels,
        levels,
        name,
        unsaved,
        collapsed,
        modal,
        view_mode,
        particles,
        preview,
        gpu,
    ) = choices;
    let chosen = tool.is_changed()
        || brush.is_changed()
        || layer.is_changed()
        || rule.is_changed()
        || panels.is_changed()
        || levels.is_changed()
        || name.is_changed()
        || unsaved.is_changed()
        || collapsed.is_changed()
        || modal.is_changed()
        || view_mode.is_changed()
        || particles.is_changed()
        || preview.is_changed()
        || gpu.is_changed();
    if input || chosen || editor.busy() || *seen != Some(view) {
        *frames_left = FRAMES_AFTER_A_CHANGE;
    }
    *seen = Some(view);
    if *frames_left > 0 {
        *frames_left -= 1;
        redraw.write(RequestRedraw);
    }
}

/// Hides a scrollbar while everything it scrolls is in sight. The toolkit draws its thumb over
/// the whole track then, a bar in the accent colour that says nothing.
fn hide_idle_scrollbars(
    mut scrollbars: Query<(&Scrollbar, &mut Visibility)>,
    areas: Query<&ComputedNode>,
) {
    for (scrollbar, mut visibility) in &mut scrollbars {
        let Ok(area) = areas.get(scrollbar.target) else {
            continue;
        };
        let (content, visible) = (area.content_size(), area.size() - area.scrollbar_size);
        // Half a physical pixel of slack for rounding.
        let scrolls = match scrollbar.orientation {
            ControlOrientation::Horizontal => content.x > visible.x + 0.5,
            ControlOrientation::Vertical => content.y > visible.y + 0.5,
        };
        let wanted = if scrolls {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// The editor's UI. The application inserts the [`Editor`](crate::session::Editor) and fills
/// the [`ViewportSlot`].
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeathersPlugins)
            .insert_resource(ui_theme())
            .init_resource::<PanelChoice>()
            .init_resource::<Tool>()
            .init_resource::<Snap>()
            .init_resource::<ViewMode>()
            .init_resource::<LookStatus>()
            .init_resource::<Brush>()
            .init_resource::<BrushAim>()
            .init_resource::<PlaceAt>()
            .init_resource::<PaintLayer>()
            .init_resource::<ChosenRule>()
            .init_resource::<Collapsed>()
            .init_resource::<RowDrag>()
            .init_resource::<Unsaved>()
            .init_resource::<Shutdown>()
            .add_observer(leave_or_ask)
            .init_resource::<LevelDialog>()
            .init_resource::<LevelNameProblem>()
            .init_resource::<Modal>()
            .init_resource::<MenuOpen>()
            .init_resource::<Particles>()
            .init_resource::<Preview>()
            .init_resource::<GpuPreview>()
            .add_observer(confirm_level_name)
            .add_systems(Startup, shell.spawn())
            .add_systems(PreUpdate, pump_editor.in_set(PumpEditor))
            .add_systems(Last, keep_drawing)
            .add_systems(
                Update,
                (
                    (fit_panels, fit_review, fit_emitters, fit_emitter).chain(),
                    (
                        show_review,
                        run_preview,
                        show_preview,
                        show_emitters,
                        (show_emitter, fill_emitter_fields).chain(),
                        fill_emitter_names,
                        commit_emitter_name,
                        show_remove_dialog,
                    ),
                    show_hierarchy,
                    (show_selection, show_drop_target).after(show_hierarchy),
                    fold_rows.after(show_hierarchy),
                    (
                        show_inspector,
                        fill_new_inputs,
                        fill_snap_steps,
                        show_mixed_marks,
                    )
                        .chain(),
                    show_status,
                    show_brush_mode,
                    show_notice,
                    (close_requested, answer_unsaved, drive_shutdown).chain(),
                    fill_names,
                    commit_name,
                    show_empty_hint,
                    (level_dialog_keys, forget_name_problem, show_level_dialog).chain(),
                    (mark_menu_open, mark_modal).chain(),
                    (
                        fold_toolbar,
                        show_tool,
                        show_duplicate,
                        show_play,
                        show_view,
                        (mirror_snap, show_snap).chain(),
                    ),
                    (keys_show_row_focus, show_row_focus).chain(),
                    hide_idle_scrollbars,
                    (
                        show_landscape_parts,
                        (show_layers, fill_layer_names).chain(),
                        commit_layer_name,
                        (show_rules, fill_rule_fields).chain(),
                    ),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_window_shows_both_side_panels() {
        assert_eq!(panels_that_fit(1440.0), (true, true));
    }

    #[test]
    fn the_inspector_gives_way_before_the_hierarchy() {
        let between = size::HIERARCHY_WIDTH + size::VIEWPORT_MIN_WIDTH + 1.0;
        assert_eq!(panels_that_fit(between), (true, false));
    }

    #[test]
    fn a_narrow_window_keeps_only_the_view() {
        // The width the tiling desktop gave the toolkit spike: 621 logical pixels.
        assert_eq!(panels_that_fit(600.0), (false, false));
        assert_eq!(panels_that_fit(621.0), (true, false));
    }
}
