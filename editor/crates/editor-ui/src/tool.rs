//! Which tool the pointer has in the 3D view, and the landscape brush's settings.
//!
//! The brush here is only what a person sets. What a stroke does to the ground (which samples
//! it touches, how the falloff shapes it, what each mode means) is decided by the editor core,
//! the same way for the preview and for the result.

use bevy::prelude::*;
use editor_core::{
    ObjectId, SnapSettings, TerrainPaintMode, TerrainPaintRequest, TerrainSculptMode,
    TerrainSculptRequest,
};
use scene_data::LayerId;

/// What a drag in the 3D view does.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tool {
    /// Objects are chosen and moved with the gizmo.
    #[default]
    Objects,
    /// The landscape is sculpted with the brush.
    Sculpt,
    /// A layer of the landscape is painted with the brush.
    Paint,
    /// The landscape's rules are set: what grows where, and which layers fill themselves.
    Rules,
    /// The level is looked over: the right-hand panel lists what the core finds wrong with
    /// it. Objects can be chosen; nothing is dragged.
    Review,
    /// The game's particle emitters are made and changed. The window shows them in the
    /// level's place: the list of emitters, the chosen one's settings, and its preview.
    Particles,
}

impl Tool {
    /// Whether the tool is one of the landscape's, which share a panel.
    pub fn is_landscape(self) -> bool {
        matches!(self, Self::Sculpt | Self::Paint | Self::Rules)
    }

    /// Whether the pointer has the brush in the 3D view.
    pub fn uses_brush(self) -> bool {
        matches!(self, Self::Sculpt | Self::Paint)
    }
}

/// How the 3D view draws the level. A choice of the window, like which panels are shown: it
/// is not part of the level, of the core's session or of what can be undone.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    /// The editor's own plain picture: what the view always was.
    #[default]
    Editor,
    /// The level lit and coloured as the game's client draws it, from the game's look profile.
    GameLook,
}

impl ViewMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Editor => "Editor",
            Self::GameLook => "Game look",
        }
    }
}

/// What the editor knows of the game's look profile: the description of sun, sky, tone and
/// materials that the game's client draws from. The application looks for it and says here
/// what it found; Game look is offered only with a profile that was loaded.
#[derive(Resource, Clone, PartialEq, Eq, Debug, Default)]
pub enum LookStatus {
    /// The editor was started without a game: there is nowhere to look for a profile.
    #[default]
    NoGame,
    /// The profile could not be used: the loader's code, the file it was looked for in, and
    /// the field at fault where the loader names one.
    Refused {
        code: String,
        path: String,
        field: Option<String>,
    },
    /// The profile is loaded. Its identity, if it has one, is what the client can be
    /// compared by.
    Loaded { identity: Option<String> },
}

impl LookStatus {
    pub fn loaded(&self) -> bool {
        matches!(self, Self::Loaded { .. })
    }
}

/// Whether a drag on the gizmo keeps to a grid, and to which. A setting of the editor, not of
/// the level: it is not saved and changes nothing until something is dragged. The editor's
/// core keeps it ([`editor_core::SnapSettings`]); this is what the core last said, for the 3D
/// view and the toolbar to read without asking.
///
/// Moving puts the dragged object on the grid's lines, on the axes it is moved along. Turning
/// goes in steps of the angle from where the object was. Scaling a box gives it a size that is
/// a whole number of grid steps, and never less than one.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct Snap {
    pub on: bool,
    /// The grid's step, in metres.
    pub grid_m: f32,
    /// The step of a turn, in degrees.
    pub angle_degrees: f32,
}

impl Default for Snap {
    /// As the core has it before anything is set: off, as the editor was before it could
    /// snap, at half a metre and fifteen degrees, the steps a level blocked out of boxes is
    /// usually built on.
    fn default() -> Self {
        SnapSettings::default().into()
    }
}

impl From<SnapSettings> for Snap {
    fn from(settings: SnapSettings) -> Self {
        Self {
            on: settings.enabled,
            grid_m: settings.translation_m,
            angle_degrees: settings.rotation_degrees,
        }
    }
}

/// How far an object must have moved along an axis to count as moved along it, in metres. A
/// drag along one axis leaves the others where they were, and they are not put on the grid.
const MOVED: f32 = 1e-4;

impl Snap {
    /// What the 3D view says about it.
    pub fn words(&self) -> String {
        if self.on {
            format!("Snap {} m, {}\u{b0}", self.grid_m, self.angle_degrees)
        } else {
            "Snap off".into()
        }
    }

    /// The step of a turn in radians, when snapping is on.
    pub fn angle_step(&self) -> Option<f32> {
        self.on.then(|| self.angle_degrees.to_radians())
    }

    fn on_grid(&self, value: f32) -> f32 {
        // Adding zero turns a negative zero into zero.
        (value / self.grid_m).round() * self.grid_m + 0.0
    }

    /// Where a dragged object goes that began at `start` and has been dragged to `now`: on the
    /// grid along every axis it moved on, and where it was along the others.
    pub fn position(&self, start: Vec3, now: Vec3) -> Vec3 {
        if !self.on {
            return now;
        }
        let axis = |start: f32, now: f32| {
            if (now - start).abs() > MOVED {
                self.on_grid(now)
            } else {
                start
            }
        };
        Vec3::new(
            axis(start.x, now.x),
            axis(start.y, now.y),
            axis(start.z, now.z),
        )
    }

    /// Where the middle of several objects goes that began at `start` and has been dragged
    /// to `now`: by a whole number of grid steps along every axis it moved on, and nowhere
    /// along the others. The middle itself is on no grid line; what was on one stays on one.
    pub fn shifted(&self, start: Vec3, now: Vec3) -> Vec3 {
        if !self.on {
            return now;
        }
        let axis = |start: f32, now: f32| {
            if (now - start).abs() > MOVED {
                start + self.on_grid(now - start)
            } else {
                start
            }
        };
        Vec3::new(
            axis(start.x, now.x),
            axis(start.y, now.y),
            axis(start.z, now.z),
        )
    }

    /// The half size of a box of `half_extents` scaled by `scale`. With snapping on, the whole
    /// size along every scaled axis is a whole number of grid steps, at least one.
    pub fn half_extents(&self, half_extents: [f32; 3], scale: Vec3) -> [f32; 3] {
        let scale = scale.to_array();
        std::array::from_fn(|axis| {
            if scale[axis] == 1.0 {
                half_extents[axis]
            } else if self.on {
                let size = self.on_grid(half_extents[axis] * scale[axis] * 2.0);
                size.max(self.grid_m) / 2.0
            } else {
                half_extents[axis] * scale[axis]
            }
        })
    }
}

/// The rule whose settings the rules tool shows, by its stable ID.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub struct ChosenRule(pub Option<String>);

/// The layer the paint tool paints, by its stable ID. `None` until a landscape has layers.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub struct PaintLayer(pub Option<String>);

/// What the brush does to the ground.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BrushMode {
    #[default]
    Raise,
    Lower,
    Smooth,
    Flatten,
}

impl BrushMode {
    pub const ALL: [BrushMode; 4] = [Self::Raise, Self::Lower, Self::Smooth, Self::Flatten];

    pub fn name(self) -> &'static str {
        match self {
            Self::Raise => "Raise",
            Self::Lower => "Lower",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
        }
    }
}

/// The brush as the person set it.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct Brush {
    pub mode: BrushMode,
    /// Radius on the ground, in metres.
    pub radius_m: f32,
    /// How strongly a stroke acts, 0 to 1. The core says what that is in metres.
    pub strength: f32,
    /// How much of the radius fades out towards the edge, 0 (hard edge) to 1 (all of it).
    pub falloff: f32,
    /// How far a stroke raises or lowers the ground at full strength, in metres.
    pub amount_m: f32,
}

impl Brush {
    /// The range the radius slider offers, in metres. Provisional: the core's own limits
    /// replace these when its brush commands exist.
    pub const RADIUS_RANGE: (f32, f32) = (0.5, 32.0);

    /// The range the height slider offers for raising and lowering, in metres.
    pub const AMOUNT_RANGE: (f32, f32) = (0.1, 5.0);

    /// The stroke along `path` on the landscape `object` as the core takes it. Flatten levels
    /// the ground to `start_height_m`: the height where the stroke began, measured in the
    /// landscape's own space, which the core works out from the first hit. Everything else
    /// about the stroke, from the samples it touches to its falloff, is the core's.
    pub fn stroke(
        &self,
        object: ObjectId,
        path: &[Vec3],
        start_height_m: f32,
    ) -> TerrainSculptRequest {
        let mode = match self.mode {
            BrushMode::Raise => TerrainSculptMode::Raise {
                amount_m: self.amount_m,
            },
            BrushMode::Lower => TerrainSculptMode::Lower {
                amount_m: self.amount_m,
            },
            BrushMode::Smooth => TerrainSculptMode::Smooth,
            BrushMode::Flatten => TerrainSculptMode::Flatten {
                height_m: start_height_m,
            },
        };
        TerrainSculptRequest {
            object,
            path_world_m: path.iter().map(Vec3::to_array).collect(),
            radius_m: self.radius_m,
            strength: self.strength,
            falloff: self.falloff,
            mode,
        }
    }

    /// The stroke along `path` that paints `layer` of the landscape `object`, or erases it.
    /// Strength is the weight the layer has at the brush's centre afterwards; what the other
    /// layers give up for it, and everything else about the stroke, is the core's.
    pub fn paint_stroke(
        &self,
        object: ObjectId,
        layer: LayerId,
        path: &[Vec3],
        erase: bool,
    ) -> TerrainPaintRequest {
        TerrainPaintRequest {
            object,
            layer,
            path_world_m: path.iter().map(Vec3::to_array).collect(),
            radius_m: self.radius_m,
            strength: self.strength,
            falloff: self.falloff,
            mode: if erase {
                TerrainPaintMode::Erase
            } else {
                TerrainPaintMode::Paint
            },
        }
    }

    /// Where the full strength ends and the falloff begins, in metres from the centre.
    pub fn inner_radius_m(&self) -> f32 {
        self.radius_m * (1.0 - self.falloff.clamp(0.0, 1.0))
    }
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            mode: BrushMode::default(),
            radius_m: 4.0,
            strength: 0.5,
            falloff: 0.5,
            amount_m: 1.0,
        }
    }
}

/// The point of a landscape the brush points at.
#[derive(Clone, PartialEq, Debug)]
pub struct BrushHit {
    pub point: Vec3,
    /// The landscape that was hit.
    pub object: ObjectId,
}

/// Where a new object is put: where the 3D view looks. The application keeps the view's
/// line of sight current; where that line meets a landscape is asked of the core when an
/// object is added, and is where the object stands.
#[derive(Resource, Default, Clone, Copy, PartialEq, Debug)]
pub struct PlaceAt {
    /// Where the view looks from.
    pub origin: Vec3,
    /// The direction it looks in.
    pub direction: Vec3,
    /// Where the object goes if the line of sight meets no landscape: the point the view
    /// turns about, on the level's zero height.
    pub fallback: Vec3,
}

/// Where the brush points on the ground, if the cursor is over a landscape.
#[derive(Resource, Default, Clone, PartialEq, Debug)]
pub struct BrushAim(pub Option<BrushHit>);

#[cfg(test)]
mod tests {
    use super::*;

    fn snapping() -> Snap {
        Snap {
            on: true,
            ..default()
        }
    }

    #[test]
    fn a_middle_is_moved_by_whole_steps_and_stays_off_the_grid_where_it_was() {
        let start = Vec3::new(1.765, 0.985, -1.03);
        let dragged = start + Vec3::new(1.17, 0.0, -0.2);
        let to = snapping().shifted(start, dragged);
        // 1.17 m is nearest to two steps of half a metre and one, 0.2 m to none.
        assert!(to.distance(start + Vec3::new(1.0, 0.0, 0.0)) < 1e-5, "{to}");
        assert_eq!(Snap::default().shifted(start, dragged), dragged);
    }

    #[test]
    fn snapping_is_off_until_it_is_switched_on_and_then_changes_nothing_by_itself() {
        let snap = Snap::default();
        assert!(!snap.on);
        let (start, now) = (Vec3::new(0.2, 1.0, 0.0), Vec3::new(1.37, 1.0, 0.0));
        assert_eq!(snap.position(start, now), now);
        assert_eq!(
            snap.half_extents([1.0, 1.0, 1.0], Vec3::new(1.3, 1.0, 1.0)),
            [1.3, 1.0, 1.0]
        );
        assert_eq!(snap.angle_step(), None);
        assert_eq!(snap.words(), "Snap off");
    }

    #[test]
    fn a_moved_object_lands_on_the_grid_only_along_the_axes_it_moved_on() {
        let snap = snapping();
        // Dragged along X from a place that is on no grid line: X goes to the grid, the height
        // and the depth stay exactly what they were.
        let start = Vec3::new(0.2, 1.37, -4.11);
        let moved = snap.position(start, Vec3::new(1.37, 1.37, -4.11));
        assert_eq!(moved, Vec3::new(1.5, 1.37, -4.11));
        // Every snapped value is a whole number of steps, exactly.
        let far = snap.position(Vec3::ZERO, Vec3::new(-7.26, 3.74, 0.26));
        assert_eq!(far, Vec3::new(-7.5, 3.5, 0.5));
        assert!(snap
            .position(Vec3::ZERO, Vec3::new(-0.1, 0.0, 0.0))
            .x
            .is_sign_positive());
    }

    #[test]
    fn a_scaled_box_gets_a_size_of_whole_grid_steps_and_never_less_than_one() {
        let snap = snapping();
        // Two metres wide, pulled to 2.6 m: the nearest whole step is 2.5 m.
        assert_eq!(
            snap.half_extents([1.0, 0.3, 2.0], Vec3::new(1.3, 1.0, 1.0)),
            [1.25, 0.3, 2.0]
        );
        // Squeezed to almost nothing: one step is left.
        assert_eq!(
            snap.half_extents([1.0, 1.0, 1.0], Vec3::new(1.0, 0.01, 1.0)),
            [1.0, 0.25, 1.0]
        );
    }

    #[test]
    fn the_view_says_the_steps_while_snapping_is_on() {
        assert_eq!(snapping().words(), "Snap 0.5 m, 15\u{b0}");
        assert_eq!(snapping().angle_step(), Some(15.0_f32.to_radians()));
    }

    #[test]
    fn the_falloff_begins_inside_the_radius() {
        let brush = Brush {
            radius_m: 8.0,
            falloff: 0.25,
            ..default()
        };
        assert_eq!(brush.inner_radius_m(), 6.0);
        assert_eq!(
            Brush {
                falloff: 0.0,
                ..brush
            }
            .inner_radius_m(),
            8.0
        );
        assert_eq!(
            Brush {
                falloff: 3.0,
                ..brush
            }
            .inner_radius_m(),
            0.0
        );
    }

    #[test]
    fn a_stroke_carries_the_settings_and_the_whole_path_in_order() {
        let brush = Brush {
            mode: BrushMode::Lower,
            radius_m: 3.0,
            strength: 0.25,
            falloff: 0.75,
            amount_m: 2.0,
        };
        let path = [Vec3::new(1.0, 0.5, 2.0), Vec3::new(4.0, 0.5, 2.0)];
        let stroke = brush.stroke(ObjectId::from_u128(7), &path, 0.5);
        assert_eq!(stroke.path_world_m, vec![[1.0, 0.5, 2.0], [4.0, 0.5, 2.0]]);
        assert_eq!(
            (stroke.radius_m, stroke.strength, stroke.falloff),
            (3.0, 0.25, 0.75)
        );
        assert_eq!(stroke.mode, TerrainSculptMode::Lower { amount_m: 2.0 });
    }

    #[test]
    fn flatten_levels_to_the_height_it_is_given_and_not_to_the_world_height_of_the_path() {
        let brush = Brush {
            mode: BrushMode::Flatten,
            ..default()
        };
        let path = [Vec3::new(0.0, 6.25, 0.0), Vec3::new(5.0, 9.0, 0.0)];
        let stroke = brush.stroke(ObjectId::from_u128(7), &path, 1.5);
        assert_eq!(stroke.mode, TerrainSculptMode::Flatten { height_m: 1.5 });
    }

    #[test]
    fn a_paint_stroke_carries_the_layer_the_path_and_the_settings_and_nothing_worked_out() {
        let brush = Brush {
            radius_m: 3.0,
            strength: 0.25,
            falloff: 0.75,
            ..default()
        };
        let path = [Vec3::new(1.0, 0.5, 2.0), Vec3::new(4.0, 0.5, 2.0)];
        let (object, layer) = (ObjectId::from_u128(7), LayerId::from_u128(9));
        let stroke = brush.paint_stroke(object.clone(), layer.clone(), &path, false);
        assert_eq!(
            (stroke.object, stroke.layer),
            (object.clone(), layer.clone())
        );
        assert_eq!(stroke.path_world_m, vec![[1.0, 0.5, 2.0], [4.0, 0.5, 2.0]]);
        assert_eq!(
            (stroke.radius_m, stroke.strength, stroke.falloff),
            (3.0, 0.25, 0.75)
        );
        assert_eq!(stroke.mode, TerrainPaintMode::Paint);
        let erase = brush.paint_stroke(object, layer, &path, true);
        assert_eq!(erase.mode, TerrainPaintMode::Erase);
    }

    #[test]
    fn the_default_brush_is_inside_the_range_the_panel_offers() {
        let brush = Brush::default();
        assert!(brush.radius_m >= Brush::RADIUS_RANGE.0 && brush.radius_m <= Brush::RADIUS_RANGE.1);
        assert!((0.0..=1.0).contains(&brush.strength));
        assert!((0.0..=1.0).contains(&brush.falloff));
        assert!(brush.amount_m >= Brush::AMOUNT_RANGE.0 && brush.amount_m <= Brush::AMOUNT_RANGE.1);
    }
}
