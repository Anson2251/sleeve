use std::{fs, path::Path};

use chrono::Local;

use crate::models::BackupVersion;

const BACKUP_DIRECTORY: &str = ".sleeve-backups";

pub fn create_backup(root: &Path, source: &Path) -> Result<BackupVersion, String> {
    let relative_path = source
        .strip_prefix(root)
        .map_err(|_| format!("文件 {} 不属于当前目录。", source.display()))?;
    let timestamp = next_backup_timestamp(root);
    let destination = root
        .join(BACKUP_DIRECTORY)
        .join(&timestamp)
        .join(relative_path);
    let parent = destination
        .parent()
        .ok_or_else(|| "无法确定备份目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建备份目录：{error}"))?;
    fs::copy(source, &destination).map_err(|error| format!("无法创建备份：{error}"))?;
    let size_bytes = fs::metadata(&destination)
        .map_err(|error| format!("无法读取备份信息：{error}"))?
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

pub fn list_backups(root: &Path, source: &Path) -> Result<Vec<BackupVersion>, String> {
    let relative_path = source
        .strip_prefix(root)
        .map_err(|_| format!("文件 {} 不属于当前目录。", source.display()))?;
    let backup_root = root.join(BACKUP_DIRECTORY);
    let entries = match fs::read_dir(&backup_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("无法读取备份目录：{error}")),
    };

    let mut versions = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let timestamp = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path().join(relative_path);
            fs::metadata(&path)
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| BackupVersion {
                    timestamp,
                    path,
                    size_bytes: metadata.len(),
                })
        })
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    Ok(versions)
}

pub fn restore_backup(root: &Path, source: &Path, backup: &Path) -> Result<(), String> {
    create_backup(root, source)?;
    fs::copy(backup, source).map_err(|error| format!("无法恢复备份：{error}"))?;
    Ok(())
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
}
