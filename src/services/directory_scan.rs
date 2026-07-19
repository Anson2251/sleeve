use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use audiotags::Tag;

use crate::models::FileTreeNode;

pub fn scan_directory(root: PathBuf) -> Result<Option<FileTreeNode>, String> {
    build_node(&root).map_err(|error| format!("无法扫描 {}：{error}", root.display()))
}

fn build_node(path: &Path) -> io::Result<Option<FileTreeNode>> {
    if path.is_file() {
        return Ok(is_supported_audio(path).then(|| FileTreeNode::audio(path.to_owned())));
    }

    let mut children = Vec::new();
    let mut direct_audio_count = 0;
    let mut albums = Vec::new();
    let mut album_cover = None;
    for entry in fs::read_dir(path)? {
        let Ok(entry) = entry else { continue };
        let entry_path = entry.path();
        if entry_path.is_file() && is_supported_audio(&entry_path) {
            direct_audio_count += 1;
            if let Some((album, cover)) = read_album_tag(&entry_path) {
                albums.push(album);
                if album_cover.is_none() {
                    album_cover = cover;
                }
            }
        }
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

    Ok((!children.is_empty()).then(|| {
        let is_album = is_album_directory(
            &display_name(path),
            direct_audio_count,
            albums.iter().map(String::as_str),
        );
        FileTreeNode::directory_with_album_cover(
            path.to_owned(),
            children,
            is_album,
            is_album.then(|| album_cover.map(Arc::new)).flatten(),
        )
    }))
}

fn read_album_tag(path: &Path) -> Option<(String, Option<Vec<u8>>)> {
    let tag = Tag::new().read_from_path(path).ok()?;
    tag.album_title().map(|album| {
        (
            album.to_owned(),
            tag.album_cover().map(|picture| picture.data.to_vec()),
        )
    })
}

fn is_album_directory<'a>(
    directory_name: &str,
    audio_file_count: usize,
    album_names: impl IntoIterator<Item = &'a str>,
) -> bool {
    if audio_file_count == 0 {
        return false;
    }

    let mut counts = HashMap::new();
    for album_name in album_names {
        let normalized = normalize_album_name(album_name);
        if !normalized.is_empty() {
            *counts.entry(normalized).or_insert(0usize) += 1;
        }
    }

    counts.into_iter().any(|(album_name, count)| {
        count * 2 > audio_file_count && album_name_similarity(directory_name, &album_name) >= 0.70
    })
}

fn album_name_similarity(directory_name: &str, album_name: &str) -> f64 {
    let directory_name = normalize_album_name(directory_name);
    let album_name = normalize_album_name(album_name);
    if directory_name.is_empty() || album_name.is_empty() {
        return 0.0;
    }
    if directory_name == album_name {
        return 1.0;
    }
    if directory_name.chars().count() < 3 || album_name.chars().count() < 3 {
        return 0.0;
    }
    if directory_name.contains(&album_name) || album_name.contains(&directory_name) {
        return 0.85;
    }

    let left = bigrams(&directory_name);
    let right = bigrams(&album_name);
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn normalize_album_name(name: &str) -> String {
    let trimmed = name.trim();
    let without_prefix = trimmed
        .split_once(['-', '_', '–', '—'])
        .filter(|(prefix, _)| {
            let prefix = prefix.trim();
            prefix.chars().all(|character| character.is_ascii_digit())
                && (prefix.len() == 4 || prefix.parse::<u32>().is_ok())
        })
        .map_or(trimmed, |(_, suffix)| suffix.trim());

    without_prefix
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn bigrams(value: &str) -> HashSet<(char, char)> {
    value.chars().zip(value.chars().skip(1)).collect()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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

    #[test]
    fn scores_normalized_album_names() {
        assert_eq!(album_name_similarity("2024 - My Album", "my_album"), 1.0);
        assert!(album_name_similarity("The Complete Collection", "Complete Collection") >= 0.85);
        assert!(album_name_similarity("A", "a") >= 1.0);
        assert_eq!(album_name_similarity("A", "AB"), 0.0);
    }

    #[test]
    fn requires_a_strict_majority_of_similar_album_tags() {
        assert!(is_album_directory(
            "2024 - My Album",
            3,
            ["My Album", "my_album", "My Album"],
        ));
        assert!(!is_album_directory(
            "My Album",
            4,
            ["My Album", "My Album", "Other Album", "Other Album"],
        ));
        assert!(!is_album_directory(
            "My Album",
            3,
            ["Unrelated", "Unrelated", "Unrelated"],
        ));
    }
}
