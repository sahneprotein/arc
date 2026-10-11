use super::dirty::terrain_dirty;
use super::*;
impl EditorSession {
    pub(super) fn apply(&mut self, command: &Command) -> Result<CommandOutcome, Diagnostic> {
        if self.transaction.is_some()
            && !matches!(
                command,
                Command::Preview { .. }
                    | Command::PreviewTerrainSculpt { .. }
                    | Command::PreviewTerrainPaint { .. }
                    | Command::CommitViewportEdit { .. }
                    | Command::CancelViewportEdit { .. }
            )
        {
            return Err(Diagnostic::new("transaction.busy"));
        }
        match command {
            Command::AssignField {
                objects,
                assignment,
                label_key,
            } => {
                let supplied: BTreeSet<_> = objects.iter().cloned().collect();
                if supplied.is_empty() || supplied.iter().ne(self.selection.iter()) {
                    return Err(Diagnostic::new("selection.stale"));
                }
                let mutations = crate::editing::assign_field(&self.document, objects, assignment)?;
                self.apply(&Command::Edit {
                    label_key: label_key.clone(),
                    mutations,
                })
            }
            Command::DuplicateObjects { copies, label_key } => {
                let crate::editing::Duplication {
                    mutations,
                    selected,
                    remap,
                } = crate::editing::duplicate_objects(&self.document, &self.selection, copies)?;
                let mut source_active = self.active.clone();
                let active = loop {
                    let Some(id) = source_active else {
                        break selected.first().cloned();
                    };
                    if let Some(copy) = remap.get(&id)
                        && selected.contains(copy)
                    {
                        break Some(copy.clone());
                    }
                    source_active = self.document.object(&id).and_then(|o| o.parent.clone());
                };
                let mut next = (*self.document).clone();
                mutate(&mut next, &mutations)?;
                self.commit(Arc::new(next), label_key.clone())?;
                self.set_selection(selected, active);
                Ok(CommandOutcome::Applied)
            }
            Command::ReparentObjects {
                objects,
                parent,
                keep,
            } => {
                let mutations =
                    crate::editing::reparent_objects(&self.document, objects, parent, *keep)?;
                self.apply(&Command::Edit {
                    label_key: "reparent".into(),
                    mutations,
                })
            }
            Command::SetActiveObject(active) => {
                if active
                    .as_ref()
                    .is_some_and(|id| !self.selection.contains(id))
                    || (active.is_none() && !self.selection.is_empty())
                {
                    return Err(Diagnostic::new("selection.active_not_selected"));
                }
                if self.active != *active {
                    self.active = active.clone();
                    self.regions.selection += 1;
                    self.view_revision += 1;
                }
                Ok(CommandOutcome::SelectionChanged)
            }
            Command::SetGizmoPivot(pivot) => {
                if self.pivot != *pivot {
                    self.pivot = *pivot;
                    self.regions.selection += 1;
                    self.view_revision += 1;
                }
                Ok(CommandOutcome::SelectionChanged)
            }
            Command::SetSnap(snap) => {
                snap.validate()?;
                if self.snap != *snap {
                    self.snap = *snap;
                    self.regions.selection += 1;
                    self.view_revision += 1;
                }
                Ok(CommandOutcome::SelectionChanged)
            }
            Command::SetEmitterAssets(known) => {
                if *self.emitters != *known {
                    self.emitters = Arc::new(known.clone());
                    self.review = review_document_with(&self.document, &self.emitters).into();
                    self.regions.review += 1;
                    self.view_revision += 1;
                }
                Ok(CommandOutcome::SelectionChanged)
            }
            Command::Edit {
                label_key,
                mutations,
            } => {
                let mut next = (*self.document).clone();
                mutate(&mut next, mutations)?;
                self.commit(Arc::new(next), label_key.clone())?;
                Ok(CommandOutcome::Applied)
            }
            Command::Select(ids) => {
                if ids.iter().any(|id| self.document.object(id).is_none()) {
                    return Err(Diagnostic::new("selection.missing"));
                }
                self.set_selection(ids.clone(), ids.last().cloned());
                Ok(CommandOutcome::SelectionChanged)
            }
            Command::Undo => {
                let h = self
                    .undo
                    .pop_back()
                    .ok_or_else(|| Diagnostic::new("history.empty"))?;
                self.replace(h.before.clone())?;
                self.redo.push_back(h);
                Ok(CommandOutcome::Applied)
            }
            Command::Redo => {
                let h = self
                    .redo
                    .pop_back()
                    .ok_or_else(|| Diagnostic::new("history.empty"))?;
                self.replace(h.after.clone())?;
                self.undo.push_back(h);
                Ok(CommandOutcome::Applied)
            }
            Command::BeginViewportEdit { selection_revision } => {
                if *selection_revision != self.regions.selection {
                    return Err(Diagnostic::new("selection.stale"));
                }
                let id = self.next_transaction;
                self.next_transaction += 1;
                self.transaction = Some(Transaction {
                    id,
                    sequence: 0,
                    base: self.document.clone(),
                    preview: self.document.clone(),
                    dirty: Arc::from([]),
                });
                self.view_revision += 1;
                Ok(CommandOutcome::TransactionStarted(id))
            }
            Command::Preview {
                transaction,
                sequence,
                mutations,
            } => {
                let t = self
                    .transaction
                    .as_mut()
                    .filter(|t| t.id == *transaction)
                    .ok_or_else(|| Diagnostic::new("transaction.stale"))?;
                if *sequence <= t.sequence {
                    return Err(Diagnostic::new("preview.stale"));
                }
                let mut next = (*t.base).clone();
                mutate(&mut next, mutations)?;
                t.dirty = terrain_dirty(&t.base, &next);
                t.preview = Arc::new(next);
                t.sequence = *sequence;
                self.view_revision += 1;
                Ok(CommandOutcome::Previewed)
            }
            Command::PreviewTerrainSculpt {
                transaction,
                sequence,
                request,
            } => {
                let base = self
                    .transaction
                    .as_ref()
                    .filter(|t| t.id == *transaction)
                    .ok_or_else(|| Diagnostic::new("transaction.stale"))?
                    .base
                    .clone();
                let current_sequence = self.transaction.as_ref().unwrap().sequence;
                if *sequence <= current_sequence {
                    return Err(Diagnostic::new("preview.stale"));
                }
                let (result, preview) = crate::sculpt::evaluate_terrain_sculpt(&base, request)?;
                let transaction = self.transaction.as_mut().unwrap();
                transaction.dirty = result.dirty_regions.clone();
                transaction.preview = preview.map_or_else(|| base, Arc::new);
                transaction.sequence = *sequence;
                self.view_revision += 1;
                Ok(CommandOutcome::TerrainSculpted {
                    sequence: *sequence,
                    result,
                })
            }
            Command::PreviewTerrainPaint {
                transaction,
                sequence,
                request,
            } => {
                let t = self
                    .transaction
                    .as_ref()
                    .filter(|t| t.id == *transaction)
                    .ok_or_else(|| Diagnostic::new("transaction.stale"))?;
                if *sequence <= t.sequence {
                    return Err(Diagnostic::new("preview.stale"));
                }
                let base = t.base.clone();
                let (result, preview) = crate::paint::evaluate_terrain_paint(&base, request)?;
                let t = self.transaction.as_mut().unwrap();
                t.dirty = result.dirty_regions.clone();
                t.preview = preview.map_or(base, Arc::new);
                t.sequence = *sequence;
                self.view_revision += 1;
                Ok(CommandOutcome::TerrainPainted {
                    sequence: *sequence,
                    result,
                })
            }
            Command::CommitViewportEdit {
                transaction,
                label_key,
            } => {
                let t = self
                    .transaction
                    .as_ref()
                    .filter(|t| t.id == *transaction)
                    .ok_or_else(|| Diagnostic::new("transaction.stale"))?;
                self.commit(t.preview.clone(), label_key.clone())?;
                self.transaction = None;
                self.view_revision += 1;
                Ok(CommandOutcome::Applied)
            }
            Command::CancelViewportEdit { transaction } => {
                if self
                    .transaction
                    .as_ref()
                    .is_none_or(|t| t.id != *transaction)
                {
                    return Err(Diagnostic::new("transaction.stale"));
                }
                self.transaction = None;
                self.view_revision += 1;
                Ok(CommandOutcome::Cancelled)
            }
        }
    }
    fn set_selection(&mut self, ids: Vec<ObjectId>, active: Option<ObjectId>) {
        self.selection = ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .into();
        self.active = active
            .filter(|id| self.selection.contains(id))
            .or_else(|| self.selection.first().cloned());
        self.inspector = inspector(&self.document, &self.selection);
        self.regions.selection += 1;
        self.regions.inspector += 1;
        self.view_revision += 1;
    }
}
