use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::models::FileTreeNode;

pub fn scan_directory(root: PathBuf) -> Result<Option<FileTreeNode>, String> {
    build_node(&root).map_err(|error| format!("无法扫描 {}：{error}", root.display()))
}

fn build_node(path: &Path) -> io::Result<Option<FileTreeNode>> {
    if path.is_file() {
        return Ok(is_supported_audio(path).then(|| FileTreeNode::audio(path.to_owned())));
    }

    let mut children = Vec::new();
    for entry in fs::read_dir(path)? {
        let Ok(entry) = entry else { continue };
        let entry_path = entry.path();
        if let Ok(Some(node)) = build_node(&entry_path) {
            children.push(node);
        }
    }

    children.sort_by(|left, right| {
        let left_directory = matches!(left, FileTreeNode::Directory { .. });
        let right_directory = matches!(right, FileTreeNode::Directory { .. });
        right_directory
            .cmp(&left_directory)
            .then_with(|| left.path().cmp(right.path()))
    });

    Ok((!children.is_empty()).then(|| FileTreeNode::directory(path.to_owned(), children)))
}

pub fn is_supported_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp3" | "flac" | "m4a" | "m4b" | "mp4")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_formats_supported_by_audiotags() {
        assert!(is_supported_audio(Path::new("track.MP3")));
        assert!(is_supported_audio(Path::new("track.flac")));
        assert!(is_supported_audio(Path::new("track.m4a")));
        assert!(!is_supported_audio(Path::new("track.ogg")));
        assert!(!is_supported_audio(Path::new("cover.jpg")));
    }
}
