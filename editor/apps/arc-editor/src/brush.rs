//! The landscape brush in the 3D view: where it points, the ring that shows it, and strokes.
//!
//! This is the pointer's side of sculpting. The ray from the cursor is tested against the
//! ground the core's renderer draws, which is the game's own collision surface, so the ring
//! sits where a stroke acts. A stroke is the ordered list of points the brush passed over;
//! the editor core works out what that does to the ground, for the preview and for the
//! result alike. One stroke is one transaction of the core and so one undo step.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
    prelude::*,
    window::{PrimaryWindow, RequestRedraw},
};
use editor_core::{terrain_hit_height_m, ObjectId, MAX_SCULPT_POINTS};
use editor_ui::{
    theme::{color, feathers::to_color},
    tool::BrushHit,
    Brush, BrushAim, Editor, Modal, PaintLayer, Tool,
};
use landscape_render::GroundTile;
use scene_data::{LayerId, StableId};

use crate::viewport::ViewCamera;

pub struct BrushPlugin;

impl Plugin for BrushPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stroke>()
            .add_systems(Update, (aim, sculpt, draw_ring).chain());
    }
}

/// What a stroke does, fixed when it begins.
enum StrokeKind {
    /// Sculpts. Flatten levels to this height, the stroke's first point in the landscape's
    /// own space.
    Sculpt { start_height: f32 },
    /// Paints this layer, or erases it.
    Paint { layer: LayerId, erase: bool },
}

/// A stroke under way.
struct Held {
    /// The tool it was begun with; choosing another ends it.
    tool: Tool,
    /// The landscape under the brush.
    object: ObjectId,
    /// The points the brush has passed over, in order.
    path: Vec<Vec3>,
    kind: StrokeKind,
}

impl Held {
    /// Sends the whole stroke so far to the core as the drag's preview.
    fn preview(&self, brush: &Brush, editor: &mut Editor) {
        match &self.kind {
            StrokeKind::Sculpt { start_height } => {
                editor.preview_sculpt(brush.stroke(self.object.clone(), &self.path, *start_height));
            }
            StrokeKind::Paint { layer, erase } => editor.preview_paint(brush.paint_stroke(
                self.object.clone(),
                layer.clone(),
                &self.path,
                *erase,
            )),
        }
    }
}

/// The stroke under way, if any.
#[derive(Resource, Default)]
pub struct Stroke {
    held: Option<Held>,
}

impl Stroke {
    pub fn is_active(&self) -> bool {
        self.held.is_some()
    }
}

/// Where the brush looks for the ground: at the cursor, then a fraction of a logical pixel to
/// either side of it.
const AIM_NUDGES: [Vec2; 3] = [Vec2::ZERO, Vec2::new(0.37, 0.19), Vec2::new(-0.23, 0.31)];

/// Finds the point of the landscape under the cursor while the sculpt tool is chosen.
fn aim(
    (tool, modal): (Res<Tool>, Res<Modal>),
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<ViewCamera>>,
    ground: Query<&GroundTile>,
    mut ray_cast: MeshRayCast,
    mut aim: ResMut<BrushAim>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    // Behind a dialog the brush points at nothing, and so cannot begin a stroke.
    let hit = (tool.uses_brush() && !modal.0)
        .then(|| window.cursor_position())
        .flatten()
        .filter(|cursor| {
            // Only inside the view's rectangle: over a panel the brush points at nothing.
            camera
                .0
                .logical_viewport_rect()
                .is_some_and(|rect| rect.contains(*cursor))
        })
        .and_then(|cursor| {
            let only_ground = |entity| ground.contains(entity);
            let settings = MeshRayCastSettings::default().with_filter(&only_ground);
            // A ray that runs exactly through a corner or an edge of the ground's triangles
            // can pass between them and meet nothing: the engine's ray cast is not watertight.
            // The cursor on such a pixel would lose the brush, so a miss is tried again a
            // fraction of a pixel to the side.
            AIM_NUDGES.iter().find_map(|nudge| {
                let ray = camera.0.viewport_to_world(camera.1, cursor + *nudge).ok()?;
                let (entity, hit) = ray_cast.cast_ray(ray, &settings).first()?;
                Some(BrushHit {
                    point: hit.point,
                    object: ground.get(*entity).ok()?.object.clone(),
                })
            })
        });
    if aim.0 != hit {
        aim.0 = hit;
        redraw.write(RequestRedraw);
    }
}

/// How far the brush must travel before the stroke gets another point: a quarter of its
/// radius, so that neighbouring stamps overlap well, and never less than a few centimetres.
fn point_spacing(brush: &Brush) -> f32 {
    (brush.radius_m * 0.25).max(0.05)
}

/// Whether the stroke takes `point` as its next point.
fn takes(path: &[Vec3], point: Vec3, spacing: f32) -> bool {
    path.len() < MAX_SCULPT_POINTS
        && path
            .last()
            .is_none_or(|last| last.distance(point) >= spacing)
}

/// Turns a drag with the sculpt or the paint brush into one transaction of the core.
///
/// The button going down on the landscape begins the stroke. Each new point sends the whole
/// stroke so far as a preview. The button coming up commits it. Escape, the window losing
/// focus and the cursor leaving the window cancel it and put the ground back.
fn sculpt(
    (tool, layer): (Res<Tool>, Res<PaintLayer>),
    brush: Res<Brush>,
    aim: Res<BrushAim>,
    (mouse, keys): (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>),
    window: Single<&Window, With<PrimaryWindow>>,
    mut stroke: ResMut<Stroke>,
    mut editor: NonSendMut<Editor>,
) {
    let Some(held) = &mut stroke.held else {
        // A stroke begins with the button going down on a landscape.
        if mouse.just_pressed(MouseButton::Left)
            && let Some(hit) = &aim.0
            && !editor.dragging()
        {
            let (kind, label) = match *tool {
                // Where the stroke began, as a height in the landscape's own space: the core
                // works it out from the level as it is before the stroke changes it.
                Tool::Sculpt => (
                    StrokeKind::Sculpt {
                        start_height: terrain_hit_height_m(
                            &editor.snapshot().document,
                            &hit.object,
                            hit.point.to_array(),
                        )
                        .unwrap_or(hit.point.y),
                    },
                    "sculpt",
                ),
                // Shift held as the stroke begins erases; the stroke keeps what it began as.
                Tool::Paint => match &layer.0 {
                    Some(layer) => (
                        StrokeKind::Paint {
                            layer: StableId(layer.clone()),
                            erase: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
                        },
                        "paint",
                    ),
                    // Without a layer there is nothing to paint.
                    None => return,
                },
                Tool::Objects | Tool::Rules | Tool::Review | Tool::Particles => return,
            };
            let held = Held {
                tool: *tool,
                object: hit.object.clone(),
                path: vec![hit.point],
                kind,
            };
            editor.begin_stroke(label);
            held.preview(&brush, &mut editor);
            stroke.held = Some(held);
        }
        return;
    };
    // The core refused the stroke and the editor gave the drag up: nothing is held any more.
    if !editor.dragging() {
        stroke.held = None;
        return;
    }
    let cancelled = keys.just_pressed(KeyCode::Escape)
        || !window.focused
        || window.cursor_position().is_none()
        || *tool != held.tool;
    if cancelled {
        editor.end_drag(false);
        stroke.held = None;
        return;
    }
    if !mouse.pressed(MouseButton::Left) {
        editor.end_drag(true);
        stroke.held = None;
        return;
    }
    // The brush moved on, over the same landscape.
    if let Some(hit) = &aim.0
        && hit.object == held.object
        && takes(&held.path, hit.point, point_spacing(&brush))
    {
        held.path.push(hit.point);
        held.preview(&brush, &mut editor);
    }
}

/// Draws the brush on the ground: the outer ring is the radius, the inner one is where the
/// falloff begins.
fn draw_ring(aim: Res<BrushAim>, brush: Res<Brush>, mut gizmos: Gizmos) {
    let Some(hit) = &aim.0 else {
        return;
    };
    // A circle lies in its own XY plane; turn it to lie on the ground, a little above it so
    // that it is not hidden by the surface it marks.
    let on_ground = Isometry3d::new(
        hit.point + Vec3::Y * 0.05,
        Quat::from_rotation_x(-FRAC_PI_2),
    );
    gizmos
        .circle(on_ground, brush.radius_m, to_color(color::TEXT_ON_ACCENT))
        .resolution(64);
    let inner = brush.inner_radius_m();
    if inner > 0.05 && inner < brush.radius_m {
        gizmos
            .circle(on_ground, inner, to_color(color::ACCENT_TEXT))
            .resolution(48);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract the brush relies on, checked against the core itself: a stroke is one undo
    /// step, and undoing it gives back the landscape as it was, not the level before it had one.
    #[test]
    fn undoing_a_stroke_restores_the_flat_landscape_and_keeps_it() {
        use editor_core::{Command, CommandOutcome, CommandRequest, EditorSession, Mutation};
        use scene_data::{
            terrain::{TerrainDocument, TileKey},
            ObjectData, ObjectRecord, RigidTransform, StableId,
        };

        let mut session = EditorSession::new(scene_data::current_map()).unwrap();
        let mut request = 0;
        let mut send = |session: &mut EditorSession, command: Command| {
            request += 1;
            let view = session.snapshot();
            session.submit(CommandRequest {
                request,
                session: view.session,
                expected_revision: view.document_revision,
                expected_inspector_revision: None,
                command,
            })
        };
        let landscape = StableId::from_u128(9001);
        let terrain = TerrainDocument::flat(
            8,
            [1.0, 1.0],
            vec![(StableId::from_u128(9002), TileKey { x: 0, z: 0 })],
        )
        .unwrap();
        let created = send(
            &mut session,
            Command::Edit {
                label_key: "create".into(),
                mutations: vec![Mutation::Create(ObjectRecord {
                    id: landscape.clone(),
                    parent: None,
                    sibling_order: 100,
                    name: "Landscape".into(),
                    local: RigidTransform::default(),
                    data: ObjectData::Terrain(terrain),
                })],
            },
        );
        assert!(created.outcome.is_ok(), "{:?}", created.outcome);
        let objects_with_landscape = session.snapshot().document.objects.len();

        let selection_revision = session.snapshot().regions.selection;
        let begun = send(
            &mut session,
            Command::BeginViewportEdit { selection_revision },
        );
        let Ok(CommandOutcome::TransactionStarted(transaction)) = begun.outcome else {
            panic!("no transaction: {:?}", begun.outcome);
        };
        let stroke = Brush::default().stroke(
            landscape.clone(),
            &[Vec3::new(3.0, 0.0, 3.0), Vec3::new(5.0, 0.0, 5.0)],
            0.0,
        );
        let previewed = send(
            &mut session,
            Command::PreviewTerrainSculpt {
                transaction,
                sequence: 1,
                request: stroke,
            },
        );
        assert!(previewed.outcome.is_ok(), "{:?}", previewed.outcome);
        let committed = send(
            &mut session,
            Command::CommitViewportEdit {
                transaction,
                label_key: "drag".into(),
            },
        );
        assert!(committed.outcome.is_ok(), "{:?}", committed.outcome);
        assert_eq!(session.snapshot().undo_label.as_deref(), Some("drag"));

        let undone = send(&mut session, Command::Undo);
        assert!(undone.outcome.is_ok(), "{:?}", undone.outcome);
        let view = session.snapshot();
        assert_eq!(
            view.document.objects.len(),
            objects_with_landscape,
            "undoing the stroke must not remove the landscape"
        );
        assert_eq!(view.undo_label.as_deref(), Some("create"));
        assert_eq!(view.redo_label.as_deref(), Some("drag"));
    }

    #[test]
    fn a_stroke_takes_a_point_only_after_the_brush_moved_far_enough() {
        let path = [Vec3::ZERO];
        assert!(!takes(&path, Vec3::new(0.5, 0.0, 0.0), 1.0));
        assert!(takes(&path, Vec3::new(1.0, 0.0, 0.0), 1.0));
        assert!(
            takes(&[], Vec3::ZERO, 1.0),
            "the first point is always taken"
        );
    }

    #[test]
    fn a_stroke_never_grows_past_what_the_core_accepts() {
        let path = vec![Vec3::ZERO; MAX_SCULPT_POINTS];
        assert!(!takes(&path, Vec3::new(100.0, 0.0, 0.0), 1.0));
    }

    #[test]
    fn points_are_a_quarter_radius_apart_and_never_closer_than_centimetres() {
        let brush = Brush {
            radius_m: 8.0,
            ..default()
        };
        assert_eq!(point_spacing(&brush), 2.0);
        assert_eq!(
            point_spacing(&Brush {
                radius_m: 0.01,
                ..brush
            }),
            0.05
        );
    }
}
