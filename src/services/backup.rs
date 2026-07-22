use std::{fs, path::Path};

use chrono::Local;

use crate::models::BackupVersion;

const BACKUP_DIRECTORY: &str = ".sleeve-backups";

pub fn create_backup(root: &Path, source: &Path) -> Result<BackupVersion, String> {
    let relative_path = source.strip_prefix(root).map_err(
        |_| crate::tf!("error.file_outside_directory", "path" => &source.display().to_string()),
    )?;
    let timestamp = next_backup_timestamp(root);
    let destination = root
        .join(BACKUP_DIRECTORY)
        .join(&timestamp)
        .join(relative_path);
    let parent = destination
        .parent()
        .ok_or_else(|| crate::t!("error.backup_directory"))?;
    fs::create_dir_all(parent).map_err(
        |error| crate::tf!("error.create_backup_directory", "error" => &error.to_string()),
    )?;
    fs::copy(source, &destination)
        .map_err(|error| crate::tf!("error.create_backup", "error" => &error.to_string()))?;
    let size_bytes = fs::metadata(&destination)
        .map_err(|error| crate::tf!("error.read_backup", "error" => &error.to_string()))?
        .len();
    Ok(BackupVersion {
        timestamp,
        path: destination,
        size_bytes,
    })
}

fn next_backup_timestamp(root: &Path) -> String {
    let base = Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let backup_root = root.join(BACKUP_DIRECTORY);
    if !backup_root.join(&base).exists() {
        return base;
    }
    for suffix in 1.. {
        let candidate = format!("{base}-{suffix:02}");
        if !backup_root.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!("unbounded suffix iterator always returns")
}

pub fn restore_snapshot(source: &Path, snapshot: &Path) -> Result<(), String> {
    fs::copy(snapshot, source)
        .map_err(|error| crate::tf!("error.restore_backup", "error" => &error.to_string()))?;
    Ok(())
}

pub fn clear_backups(root: &Path) -> Result<(), String> {
    let backup_root = root.join(BACKUP_DIRECTORY);
    match fs::remove_dir_all(&backup_root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(crate::tf!("error.clear_backups", "error" => &error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backups_preserve_the_source_relative_path() {
        let root = std::env::temp_dir().join(format!("sleeve-backup-test-{}", std::process::id()));
        let source = root.join("Album/track.mp3");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"original").unwrap();

        let backup = create_backup(&root, &source).unwrap();
        assert!(backup.path.ends_with("Album/track.mp3"));
        assert_eq!(fs::read(&backup.path).unwrap(), b"original");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restores_snapshots_without_creating_another_backup() {
        let root = std::env::temp_dir().join(format!("sleeve-restore-test-{}", std::process::id()));
        let source = root.join("track.mp3");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"original").unwrap();
        let snapshot = create_backup(&root, &source).unwrap();
        fs::write(&source, b"updated").unwrap();

        restore_snapshot(&source, &snapshot.path).unwrap();

        assert_eq!(fs::read(&source).unwrap(), b"original");
        assert!(snapshot.path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clears_the_session_backup_directory() {
        let root = std::env::temp_dir().join(format!("sleeve-clear-test-{}", std::process::id()));
        let source = root.join("track.mp3");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"original").unwrap();
        create_backup(&root, &source).unwrap();

        clear_backups(&root).unwrap();

        assert!(!root.join(BACKUP_DIRECTORY).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
