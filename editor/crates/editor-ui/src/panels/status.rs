//! The status bar: what state the level is in, always in words.

use bevy::{
    feathers::{
        display::label,
        theme::{ThemeBackgroundColor, ThemeTextColor},
        tokens,
    },
    gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSettings},
    prelude::*,
};
use editor_core::JobState;

use crate::{
    session::Editor,
    theme::{
        feathers::{SIGNAL_ERROR, SIGNAL_WARN},
        size, spacing,
    },
    tool::{Brush, BrushAim, Tool},
    wording,
};

/// One of the texts of the status bar.
#[derive(Component, Clone, Copy, PartialEq, Eq, FromTemplate)]
pub enum StatusPart {
    #[default]
    Level,
    Saved,
    Work,
    Problem,
    Cooked,
}

pub fn status_bar() -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::STATUSBAR_HEIGHT),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_6),
            padding: UiRect::horizontal(px(spacing::SPACE_4)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [
            (label("") StatusPart::Level),
            (label("") StatusPart::Saved),
            (label("") StatusPart::Work),
            (label("") StatusPart::Problem ThemeTextColor(SIGNAL_ERROR)),
            Node { flex_grow: 1.0 },
            (label("") StatusPart::Cooked),
        ]
    }
}

/// Everything the status bar says, so that it is rewritten only when a part changed.
#[derive(PartialEq, Eq, Clone, Default)]
pub struct StatusWords {
    level: String,
    saved: String,
    attention: bool,
    work: String,
    problem: String,
    cooked: String,
}

fn words(
    editor: &Editor,
    gizmo: TransformGizmoMode,
    tool: Tool,
    brush: &Brush,
    aim: &BrushAim,
) -> StatusWords {
    let snapshot = editor.snapshot();
    let status = &snapshot.status;
    let (saved, attention) = wording::saved_state(status.dirty, status.cook_needed);
    let level = editor
        .level_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let selected = match snapshot.selection.len() {
        0 => String::new(),
        n => format!("   {n} selected"),
    };
    let tool = match (tool, gizmo) {
        (Tool::Sculpt, _) => match &aim.0 {
            // The height the brush points at, to the millimetre.
            Some(hit) => format!(
                "Sculpt, {}   height {:.3} m",
                brush.mode.name(),
                hit.point.y + 0.0
            ),
            None => format!("Sculpt, {}", brush.mode.name()),
        },
        (Tool::Paint, _) => "Paint".into(),
        (Tool::Rules, _) => "Rules".into(),
        (Tool::Objects, TransformGizmoMode::Translate) => "Move".into(),
        (Tool::Objects, TransformGizmoMode::Rotate) => "Rotate".into(),
        (Tool::Objects, TransformGizmoMode::Scale) => "Scale".into(),
    };
    let work = if snapshot
        .jobs
        .iter()
        .any(|job| matches!(job.state, JobState::Running))
    {
        "Working\u{2026}".to_owned()
    } else {
        String::new()
    };
    let problem = editor
        .problem
        .as_ref()
        .map_or_else(String::new, |diagnostic| {
            let shown = wording::problem(&diagnostic.code, |key| {
                diagnostic.parameters.get(key).map(String::as_str)
            });
            format!("{}. {}", shown.title, shown.remedy)
        });
    let cooked = status.simulation_hash.as_ref().map_or_else(
        || "not cooked".to_owned(),
        |hash| format!("cooked {}", wording::short_hash(&hash.hex())),
    );
    StatusWords {
        level: format!("{level}{selected}   Tool: {tool}"),
        saved: saved.into(),
        attention,
        work,
        problem,
        cooked,
    }
}

pub(crate) fn show_status(
    mut commands: Commands,
    editor: NonSend<Editor>,
    gizmo: Res<TransformGizmoSettings>,
    (tool, brush, aim): (Res<Tool>, Res<Brush>, Res<BrushAim>),
    mut shown: Local<Option<StatusWords>>,
    mut parts: Query<(Entity, &StatusPart, &mut Text)>,
) {
    if parts.is_empty() {
        return;
    }
    let now = words(&editor, gizmo.mode, *tool, &brush, &aim);
    if shown.as_ref() == Some(&now) {
        return;
    }
    for (entity, part, mut text) in &mut parts {
        let words = match part {
            StatusPart::Level => &now.level,
            StatusPart::Saved => &now.saved,
            StatusPart::Work => &now.work,
            StatusPart::Problem => &now.problem,
            StatusPart::Cooked => &now.cooked,
        };
        text.0.clone_from(words);
        if *part == StatusPart::Saved {
            let color = if now.attention {
                SIGNAL_WARN
            } else {
                tokens::TEXT_MAIN
            };
            commands.entity(entity).insert(ThemeTextColor(color));
        }
    }
    *shown = Some(now);
}
