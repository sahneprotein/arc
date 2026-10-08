//! The notice: a problem that stays on screen until it is dealt with.
//!
//! It sits in a corner of the 3D view. It says what happened, how to fix it, and offers the
//! action that fits. The level stays as it was behind it. The words come from `wording.rs`.

use bevy::{
    feathers::{
        containers::pane_body,
        controls::{ButtonVariant, FeathersButton},
        display::{label, label_dim},
        theme::{ThemeBorderColor, ThemedText},
    },
    prelude::*,
    ui_widgets::Activate,
};

use crate::{
    session::Editor,
    theme::{feathers::SIGNAL_ERROR, spacing},
    wording,
};

/// The notice.
#[derive(Component, Default, Clone)]
pub struct Notice;

/// One of the notice's two texts.
#[derive(Component, Clone, Copy, PartialEq, Eq, FromTemplate)]
pub enum NoticeText {
    #[default]
    Title,
    Remedy,
}

/// The button that tries the failed save again. Shown only for a failed save.
#[derive(Component, Default, Clone)]
pub struct RetryButton;

/// The notice, to be placed inside the node of the 3D view.
pub fn notice() -> impl Scene {
    bsn! {
        pane_body()
        Notice
        Node {
            position_type: PositionType::Absolute,
            top: px(spacing::SPACE_4),
            right: px(spacing::SPACE_4),
            max_width: px(380),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            display: Display::None,
        }
        ThemeBorderColor(SIGNAL_ERROR)
        Children [
            (label("") NoticeText::Title),
            (label_dim("") NoticeText::Remedy),
            (
                Node { flex_direction: FlexDirection::Row, column_gap: px(spacing::SPACE_4) }
                Children [
                    (
                        @FeathersButton {
                            @caption: bsn! { Text("Try again") ThemedText },
                            @variant: ButtonVariant::Primary,
                        }
                        RetryButton
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.retry_save())
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Dismiss") ThemedText } }
                        on(|_: On<Activate>, mut editor: NonSendMut<Editor>| editor.dismiss_problem())
                    ),
                ]
            ),
        ]
    }
}

/// What the notice says now; `None` hides it.
#[derive(PartialEq, Eq, Clone)]
pub struct NoticeWords {
    title: String,
    remedy: String,
    retry: bool,
}

/// Shows the editor's problem, or hides the notice when there is none.
pub(crate) fn show_notice(
    editor: NonSend<Editor>,
    mut notice: Query<&mut Node, (With<Notice>, Without<RetryButton>)>,
    mut texts: Query<(&NoticeText, &mut Text)>,
    mut retry: Query<&mut Node, (With<RetryButton>, Without<Notice>)>,
    mut shown: Local<Option<Option<NoticeWords>>>,
) {
    let now = editor.problem.as_ref().map(|diagnostic| {
        let words = wording::problem(&diagnostic.code, |key| {
            diagnostic.parameters.get(key).map(String::as_str)
        });
        NoticeWords {
            title: words.title,
            remedy: words.remedy,
            retry: editor.problem_is_from_save(),
        }
    });
    if shown.as_ref() == Some(&now) || notice.is_empty() {
        return;
    }
    let display = |visible: bool| {
        if visible {
            Display::Flex
        } else {
            Display::None
        }
    };
    for mut node in &mut notice {
        node.display = display(now.is_some());
    }
    if let Some(words) = &now {
        for (which, mut text) in &mut texts {
            text.0.clone_from(match which {
                NoticeText::Title => &words.title,
                NoticeText::Remedy => &words.remedy,
            });
        }
        for mut node in &mut retry {
            node.display = display(words.retry);
        }
    }
    *shown = Some(now);
}
