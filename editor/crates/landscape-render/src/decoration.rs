//! Loaded CPU instances and per-view visibility. GPU counts remain unavailable until observed.
use scene_data::*;
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecorationFacts {
    pub cpu_allocated_instances: usize,
    pub submitted_instances: Option<usize>,
    pub gpu_visible_instances: Option<usize>,
    pub enabled: bool,
}
pub struct DecorationView {
    enabled: bool,
    instances: Arc<[(ObjectId, DecorationInstance)]>,
}
impl Default for DecorationView {
    fn default() -> Self {
        Self {
            enabled: true,
            instances: Arc::from([]),
        }
    }
}
impl DecorationView {
    pub fn load(
        &mut self,
        visual: &VisualPayload,
        ground: &[CookedGround],
    ) -> Result<(), Diagnostic> {
        visual.validate(ground)?;
        self.instances = visual
            .terrain
            .iter()
            .flat_map(|t| t.decoration.iter().map(|d| (t.object.clone(), d.clone())))
            .collect::<Vec<_>>()
            .into();
        Ok(())
    }
    pub fn set_enabled(&mut self, value: bool) {
        self.enabled = value;
    }
    pub fn instances(&self) -> &[(ObjectId, DecorationInstance)] {
        &self.instances
    }
    pub fn facts(&self) -> DecorationFacts {
        DecorationFacts {
            cpu_allocated_instances: self.instances.len(),
            submitted_instances: None,
            gpu_visible_instances: None,
            enabled: self.enabled,
        }
    }
}
