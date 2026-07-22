use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    models::{AudioFile, BackupVersion},
    services::{create_backup, read_audio_file, restore_snapshot},
};

#[derive(Debug)]
pub(crate) struct HistoryBatch {
    pub(super) snapshots: Vec<(PathBuf, BackupVersion)>,
}

impl HistoryBatch {
    fn paths(&self) -> BTreeSet<PathBuf> {
        self.snapshots
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }
}

#[derive(Debug, Default)]
pub(super) struct BatchHistory {
    undo: Vec<HistoryBatch>,
    redo: Vec<HistoryBatch>,
}

pub(super) fn is_current_batch_draft_result(
    result_paths: &BTreeSet<PathBuf>,
    result_selection_revision: u64,
    result_batch_draft_revision: u64,
    selected_paths: &BTreeSet<PathBuf>,
    selection_revision: u64,
    batch_draft_revision: u64,
) -> bool {
    result_paths == selected_paths
        && result_paths.len() > 1
        && result_selection_revision == selection_revision
        && result_batch_draft_revision == batch_draft_revision
}

impl BatchHistory {
    pub(super) fn can_undo(&self, selected_paths: &BTreeSet<PathBuf>) -> bool {
        self.undo
            .last()
            .is_some_and(|batch| batch.paths() == *selected_paths)
    }

    pub(super) fn can_redo(&self, selected_paths: &BTreeSet<PathBuf>) -> bool {
        self.redo
            .last()
            .is_some_and(|batch| batch.paths() == *selected_paths)
    }

    pub(super) fn record_save(&mut self, batch: HistoryBatch) {
        self.undo.push(batch);
        self.redo.clear();
    }

    pub(super) fn take(
        &mut self,
        selected_paths: &BTreeSet<PathBuf>,
        is_undo: bool,
    ) -> Option<HistoryBatch> {
        let stack = if is_undo {
            &mut self.undo
        } else {
            &mut self.redo
        };
        stack
            .last()
            .is_some_and(|batch| batch.paths() == *selected_paths)
            .then(|| stack.pop())
            .flatten()
    }

    pub(super) fn restore_failed(&mut self, batch: HistoryBatch, is_undo: bool) {
        if is_undo {
            self.undo.push(batch);
        } else {
            self.redo.push(batch);
        }
    }

    pub(super) fn complete_restore(&mut self, current: HistoryBatch, is_undo: bool) {
        if is_undo {
            self.redo.push(current);
        } else {
            self.undo.push(current);
        }
    }
}

pub(super) fn restore_history_batch(
    root: &Path,
    batch: &HistoryBatch,
) -> Result<(Vec<AudioFile>, HistoryBatch), String> {
    let mut restored_files = Vec::new();
    let mut current_snapshots = Vec::new();
    for (path, snapshot) in &batch.snapshots {
        let current_snapshot = match create_backup(root, path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                rollback_history_batch(&current_snapshots)?;
                return Err(error);
            }
        };
        current_snapshots.push((path.clone(), current_snapshot));
        if let Err(error) = restore_snapshot(path, &snapshot.path) {
            rollback_history_batch(&current_snapshots)?;
            return Err(error);
        }
        match read_audio_file(path.clone(), root.to_owned()) {
            Ok(file) => restored_files.push(file),
            Err(error) => {
                rollback_history_batch(&current_snapshots)?;
                return Err(error);
            }
        }
    }
    Ok((
        restored_files,
        HistoryBatch {
            snapshots: current_snapshots,
        },
    ))
}

pub(super) fn rollback_history_batch(snapshots: &[(PathBuf, BackupVersion)]) -> Result<(), String> {
    let errors = snapshots
        .iter()
        .rev()
        .filter_map(|(path, snapshot)| restore_snapshot(path, &snapshot.path).err())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("回滚失败：{}", errors.join("；")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_history_moves_complete_batch_between_undo_and_redo() {
        let first = PathBuf::from("first.flac");
        let second = PathBuf::from("second.flac");
        let selected = BTreeSet::from([first.clone(), second.clone()]);
        let mut history = BatchHistory::default();
        let batch = HistoryBatch {
            snapshots: vec![
                (first, backup("first-before.flac")),
                (second, backup("second-before.flac")),
            ],
        };

        history.redo.push(HistoryBatch {
            snapshots: vec![(PathBuf::from("stale.flac"), backup("stale.flac"))],
        });
        history.record_save(batch);

        assert!(history.redo.is_empty());
        assert!(history.can_undo(&selected));
        let undo_batch = history.take(&selected, true).expect("undo batch");
        assert!(history.undo.is_empty());
        history.complete_restore(undo_batch, true);
        assert!(history.can_redo(&selected));
    }

    #[test]
    fn failed_batch_restore_returns_the_batch_to_its_original_stack() {
        let path = PathBuf::from("track.flac");
        let selected = BTreeSet::from([path.clone()]);
        let batch = HistoryBatch {
            snapshots: vec![(path, backup("before.flac"))],
        };
        let mut history = BatchHistory::default();
        history.record_save(batch);
        let in_flight = history.take(&selected, true).expect("undo batch");

        history.restore_failed(in_flight, true);

        assert!(history.can_undo(&selected));
        assert!(history.redo.is_empty());
    }

    #[test]
    fn batch_draft_result_rejects_stale_edit_revision() {
        let paths = BTreeSet::from([PathBuf::from("first.flac"), PathBuf::from("second.flac")]);

        assert!(!is_current_batch_draft_result(&paths, 3, 4, &paths, 3, 5));
        assert!(!is_current_batch_draft_result(&paths, 2, 5, &paths, 3, 5));
        assert!(is_current_batch_draft_result(&paths, 3, 5, &paths, 3, 5));
    }

    #[test]
    fn batch_undo_requires_an_exact_selection_match() {
        let first = PathBuf::from("first.flac");
        let second = PathBuf::from("second.flac");
        let mut history = BatchHistory::default();
        history.record_save(HistoryBatch {
            snapshots: vec![
                (first.clone(), backup("first-backup.flac")),
                (second.clone(), backup("second-backup.flac")),
            ],
        });

        assert!(!history.can_undo(&BTreeSet::from([first.clone()])));
        assert!(
            history
                .take(&BTreeSet::from([first.clone()]), true)
                .is_none()
        );
        assert_eq!(history.undo.len(), 1);
        assert!(history.can_undo(&BTreeSet::from([first, second])));
    }

    fn backup(path: &str) -> BackupVersion {
        BackupVersion {
            timestamp: "now".into(),
            path: PathBuf::from(path),
            size_bytes: 1,
        }
    }
}
