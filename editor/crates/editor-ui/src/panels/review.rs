//! The Review panel: what the editor's core finds wrong with the level, in words.
//!
//! It takes the inspector's place while the Review tool is chosen. Everything it lists comes
//! from the core's review of the open level and from nowhere else: what stops the game from
//! running the level, by the same account the core's check before a start goes by, and what
//! is wrong with the level itself. The panel adds no finding of its own; whether the level is
//! cooked, and whether there is a game to start, are said by the Play control.
//!
//! Each problem says what it is and what to do, and where it is about one object, a button
//! selects that object. A group without problems says so.

use bevy::{
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::FeathersButton,
        display::{label, label_dim},
        theme::{ThemeBorderColor, ThemedText},
    },
    prelude::*,
    ui_widgets::Activate,
};
use editor_core::{ObjectId, ReviewScope, StableId, ViewSnapshot};

use crate::{
    panels::inspector::group_title,
    session::Editor,
    shell::PanelChoice,
    theme::{feathers::SIGNAL_WARN, size, spacing},
    tool::Tool,
    wording,
};

/// The Review panel.
#[derive(Component, Default, Clone)]
pub struct ReviewPanel;

/// The node the problems are rebuilt in.
#[derive(Component, Default, Clone)]
pub(crate) struct ReviewBody;

/// The block of one problem.
#[derive(Component, Default, Clone)]
pub struct ReviewRow;

/// The button of a problem that selects the object it is about, by that object's stable ID.
#[derive(Component, Default, Clone)]
pub struct ReviewSelect(pub String);

/// What the group of problems that stop the game says when it is empty, and the group of
/// problems with the level itself.
pub const NOTHING_STOPS_PLAY: &str = "Nothing in the level stops the game from running it.";
pub const NO_LEVEL_PROBLEMS: &str = "The level itself has no problems.";

/// One problem of the core's review as the panel shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewItem {
    pub scope: ReviewScope,
    /// The core's code, for the log and the checks. The panel shows words.
    pub code: String,
    pub title: String,
    pub remedy: String,
    /// The object the problem is about, if it is about one that is in the level: its ID and
    /// its name.
    pub object: Option<(ObjectId, String)>,
}

/// The problems of the core's review, in the core's order, in words.
pub fn review_items(snapshot: &ViewSnapshot) -> Vec<ReviewItem> {
    snapshot
        .review
        .iter()
        .map(|problem| {
            let diagnostic = &problem.diagnostic;
            let words = wording::review(&diagnostic.code, |key| {
                diagnostic.parameters.get(key).map(String::as_str)
            });
            ReviewItem {
                scope: problem.scope,
                code: diagnostic.code.clone(),
                title: words.title,
                remedy: words.remedy,
                object: diagnostic
                    .object
                    .as_ref()
                    .and_then(|id| snapshot.document.object(id))
                    .map(|object| (object.id.clone(), object.name.clone())),
            }
        })
        .collect()
}

/// The title of a group: its name, and how many problems it holds if it holds any.
pub fn group_words(name: &str, count: usize) -> String {
    match count {
        0 => name.to_owned(),
        count => format!("{name} ({count})"),
    }
}

pub fn review_panel() -> impl Scene {
    bsn! {
        pane()
        ReviewPanel
        Node { width: px(size::INSPECTOR_WIDTH), flex_shrink: 0.0, display: Display::None }
        Children [
            (pane_header() Children [ (Text("Review") ThemedText) ]),
            (
                pane_body()
                ReviewBody
                Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::clip() }
            ),
        ]
    }
}

fn select_object(
    activate: On<Activate>,
    buttons: Query<&ReviewSelect>,
    mut editor: NonSendMut<Editor>,
) {
    if let Ok(button) = buttons.get(activate.entity) {
        editor.select(vec![StableId(button.0.clone())]);
    }
}

/// The block of one problem: what it is, what to do, and the button that selects its object.
/// It is edged in the warning colour, which always comes with these words.
fn item_block(item: &ReviewItem) -> Box<dyn SceneList> {
    let title = item.title.clone();
    let remedy = item.remedy.clone();
    let select: Box<dyn SceneList> = match &item.object {
        Some((id, name)) => {
            let caption = format!("Select {name}");
            let id = id.0.clone();
            Box::new(bsn_list![(
                @FeathersButton { @caption: bsn! { Text({caption}) ThemedText } }
                ReviewSelect({id})
                Node { align_self: AlignSelf::FlexStart }
                on(select_object)
            )])
        }
        None => Box::new(bsn_list![]),
    };
    Box::new(bsn_list![(
        ReviewRow
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(spacing::SPACE_1),
            padding: UiRect::left(px(spacing::SPACE_4)),
            border: UiRect::left(px(2)),
            flex_shrink: 0.0,
        }
        ThemeBorderColor(SIGNAL_WARN)
        Children [ label(title), label_dim(remedy), {select} ]
    )])
}

/// Shows the core's review. Rebuilt when the review changed, and when the hierarchy did: a
/// problem names its object.
pub(crate) fn show_review(
    mut commands: Commands,
    editor: NonSend<Editor>,
    body: Query<Entity, With<ReviewBody>>,
    mut shown: Local<Option<(u64, u64, u64)>>,
) {
    let Ok(body) = body.single() else {
        return;
    };
    let snapshot = editor.snapshot();
    let key = (
        snapshot.session,
        snapshot.regions.review,
        snapshot.regions.hierarchy,
    );
    if *shown == Some(key) {
        return;
    }
    *shown = Some(key);
    let items = review_items(&snapshot);
    let mut scenes: Vec<Box<dyn SceneList>> = Vec::new();
    for (scope, name, none) in [
        (ReviewScope::Play, "Play", NOTHING_STOPS_PLAY),
        (ReviewScope::Scene, "Level", NO_LEVEL_PROBLEMS),
    ] {
        let of_scope: Vec<&ReviewItem> = items.iter().filter(|item| item.scope == scope).collect();
        let title = group_words(name, of_scope.len());
        scenes.push(Box::new(bsn_list![group_title(&title)]));
        if of_scope.is_empty() {
            scenes.push(Box::new(bsn_list![label_dim(none)]));
        }
        scenes.extend(of_scope.into_iter().map(item_block));
    }
    commands
        .entity(body)
        .despawn_children()
        .queue_spawn_related_scenes::<Children>(scenes);
}

/// Shows the panel in the right-hand place while the Review tool is chosen and that place is
/// shown.
pub(crate) fn fit_review(
    tool: Res<Tool>,
    choice: Res<PanelChoice>,
    mut panels: Query<&mut Node, With<ReviewPanel>>,
) {
    let wanted = if choice.shown().1 && *tool == Tool::Review {
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

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{Diagnostic, EditorSession, ReviewProblem};

    #[test]
    fn a_group_says_how_many_problems_it_holds_and_nothing_when_it_holds_none() {
        assert_eq!(group_words("Play", 0), "Play");
        assert_eq!(group_words("Play", 2), "Play (2)");
    }

    #[test]
    fn the_games_map_has_nothing_to_review() {
        let session = EditorSession::new(scene_data::current_map()).unwrap();
        assert_eq!(review_items(&session.snapshot()), []);
    }

    #[test]
    fn a_level_without_a_spawn_point_is_listed_under_play_in_words_and_names_no_object() {
        let mut map = scene_data::current_map();
        map.objects
            .retain(|object| !matches!(object.data, scene_data::ObjectData::Spawn { .. }));
        let session = EditorSession::new(map).unwrap();
        let items = review_items(&session.snapshot());
        let [item] = items.as_slice() else {
            panic!("one problem, not {items:?}");
        };
        assert_eq!(
            (item.scope, item.code.as_str()),
            (ReviewScope::Play, "play.spawn_required")
        );
        assert!(item.title.contains("no spawn point"), "{}", item.title);
        assert!(item.remedy.contains("Add"), "{}", item.remedy);
        assert_eq!(item.object, None);
    }

    #[test]
    fn a_problem_about_an_object_names_it_and_one_about_an_object_that_is_gone_names_none() {
        let session = EditorSession::new(scene_data::current_map()).unwrap();
        let mut snapshot = (*session.snapshot()).clone();
        let wall = snapshot.hierarchy[10].id.clone();
        snapshot.review = vec![
            ReviewProblem {
                scope: ReviewScope::Scene,
                diagnostic: Diagnostic::new("transform.nonfinite").at(&wall),
            },
            ReviewProblem {
                scope: ReviewScope::Play,
                diagnostic: Diagnostic::new("play.terrain_unsupported")
                    .with("reason", "rules")
                    .with("count", 2)
                    .at(&StableId("gone".into())),
            },
        ]
        .into();
        let items = review_items(&snapshot);
        assert_eq!(items[0].scope, ReviewScope::Scene);
        assert_eq!(items[0].object, Some((wall, "Wall".to_owned())));
        assert!(!items[0].title.contains("transform."), "{}", items[0].title);
        assert_eq!(items[1].object, None);
        assert!(items[1].remedy.contains("2 rules"), "{}", items[1].remedy);
        for item in &items {
            assert!(!item.title.contains(&item.code) && !item.remedy.contains(&item.code));
        }
    }
}
