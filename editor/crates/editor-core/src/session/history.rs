use super::*;
impl EditorSession {
    pub(super) fn commit(
        &mut self,
        next: Arc<SceneDocument>,
        label: String,
    ) -> Result<(), Diagnostic> {
        let hash = scene_data::source_hash(&next)?;
        if hash == self.hash {
            return Ok(());
        }
        let bytes = scene_bytes(&self.document).saturating_add(scene_bytes(&next));
        if bytes > HISTORY_BYTES {
            return Err(Diagnostic::new("history.capacity"));
        }
        let h = History {
            before: self.document.clone(),
            after: next.clone(),
            label,
            bytes,
        };
        self.replace(next)?;
        self.redo.clear();
        self.undo.push_back(h);
        while self.undo.len() > HISTORY_LIMIT
            || self.undo.iter().map(|h| h.bytes).sum::<usize>() > HISTORY_BYTES
        {
            self.undo.pop_front();
        }
        Ok(())
    }
    pub(super) fn replace(&mut self, next: Arc<SceneDocument>) -> Result<(), Diagnostic> {
        let hash = scene_data::source_hash(&next)?;
        let before: BTreeMap<_, _> = self.document.objects.iter().map(|o| (&o.id, o)).collect();
        let after: BTreeMap<_, _> = next.objects.iter().map(|o| (&o.id, o)).collect();
        self.changed = before
            .keys()
            .chain(after.keys())
            .filter(|id| before.get(*id) != after.get(*id))
            .map(|id| (*id).clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .into();
        let hierarchy_changed =
            self.changed
                .iter()
                .any(|id| match (before.get(id), after.get(id)) {
                    (Some(a), Some(b)) => {
                        a.name != b.name
                            || a.parent != b.parent
                            || a.sibling_order != b.sibling_order
                    }
                    _ => true,
                });
        if hierarchy_changed {
            self.hierarchy = hierarchy(&next);
            self.regions.hierarchy += 1;
        }
        let selection: Arc<[ObjectId]> = self
            .selection
            .iter()
            .filter(|id| after.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>()
            .into();
        if selection != self.selection {
            self.regions.selection += 1;
        }
        if selection != self.selection || self.changed.iter().any(|id| selection.contains(id)) {
            self.inspector = inspector(&next, &selection);
            self.regions.inspector += 1;
        }
        self.selection = selection;
        if self
            .active
            .as_ref()
            .is_none_or(|id| !self.selection.contains(id))
        {
            self.active = self.selection.first().cloned();
        }
        self.review = review_document(&next).into();
        self.regions.review += 1;
        self.document = next;
        self.hash = hash;
        self.revision += 1;
        self.view_revision += 1;
        self.regions.status += 1;
        self.status.cook_needed = true;
        Ok(())
    }
}

fn scene_bytes(scene: &SceneDocument) -> usize {
    scene
        .objects
        .iter()
        .map(|o| {
            512 + o.name.len()
                + match &o.data {
                    ObjectData::Terrain(t) => {
                        t.samples.len() * 32
                            + t.tiles
                                .iter()
                                .map(|t| {
                                    256 + t
                                        .layers
                                        .iter()
                                        .map(|l| l.weights.len() * 4 + 256)
                                        .sum::<usize>()
                                })
                                .sum::<usize>()
                            + t.layers.len() * 512
                    }
                    _ => 0,
                }
        })
        .sum()
}
