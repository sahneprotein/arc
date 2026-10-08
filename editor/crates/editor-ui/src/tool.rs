//! Which tool the pointer has in the 3D view, and the landscape brush's settings.
//!
//! The brush here is only what a person sets. What a stroke does to the ground (which samples
//! it touches, how the falloff shapes it, what each mode means) is decided by the editor core,
//! the same way for the preview and for the result.

use bevy::prelude::*;
use editor_core::{
    ObjectId, TerrainPaintMode, TerrainPaintRequest, TerrainSculptMode, TerrainSculptRequest,
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
