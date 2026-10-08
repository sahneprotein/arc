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
                let ids: BTreeSet<_> = ids.iter().cloned().collect();
                if ids.iter().any(|id| self.document.object(id).is_none()) {
                    return Err(Diagnostic::new("selection.missing"));
                }
                self.selection = ids.into_iter().collect::<Vec<_>>().into();
                self.inspector = inspector(&self.document, &self.selection);
                self.regions.selection += 1;
                self.regions.inspector += 1;
                self.view_revision += 1;
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
}
