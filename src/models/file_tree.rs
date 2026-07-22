use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileTreeNode {
    Directory {
        name: String,
        path: PathBuf,
        children: Vec<FileTreeNode>,
        is_album: bool,
        album_cover: Option<Arc<Vec<u8>>>,
    },
    AudioFile {
        name: String,
        path: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub name: String,
    pub path: PathBuf,
    pub depth: usize,
    pub is_directory: bool,
    pub is_album: bool,
    pub album_cover: Option<Arc<Vec<u8>>>,
    pub expanded: bool,
}

impl FileTreeNode {
    pub fn directory_with_album_cover(
        path: PathBuf,
        children: Vec<Self>,
        is_album: bool,
        album_cover: Option<Arc<Vec<u8>>>,
    ) -> Self {
        let mut names = vec![display_name(&path)];
        let mut path = path;
        let mut children = children;
        let mut is_album = is_album;
        let mut album_cover = album_cover;

        while let [
            Self::Directory {
                name,
                path: child_path,
                children: child_children,
                is_album: child_is_album,
                album_cover: child_album_cover,
            },
        ] = children.as_slice()
        {
            let (child_name, next_path, next_children, next_is_album, next_album_cover) = (
                name.clone(),
                child_path.clone(),
                child_children.clone(),
                *child_is_album,
                child_album_cover.clone(),
            );
            names.push(child_name);
            path = next_path;
            children = next_children;
            is_album = next_is_album;
            album_cover = next_album_cover;
        }

        Self::Directory {
            name: names.join("/"),
            path,
            children,
            is_album,
            album_cover,
        }
    }

    pub fn audio(path: PathBuf) -> Self {
        Self::AudioFile {
            name: display_name(&path),
            path,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Directory { path, .. } | Self::AudioFile { path, .. } => path,
        }
    }

    pub fn flatten(&self, expanded_paths: &[PathBuf]) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        match self {
            Self::Directory { children, .. } => {
                for child in children {
                    child.append_rows(0, expanded_paths, &mut rows);
                }
            }
            Self::AudioFile { .. } => self.append_rows(0, expanded_paths, &mut rows),
        }
        rows
    }

    pub fn album_directory_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        self.append_album_directory_paths(&mut paths);
        paths
    }

    pub fn update_album_cover_for_file(
        &mut self,
        file_path: &Path,
        cover: Option<Arc<Vec<u8>>>,
    ) -> Option<PathBuf> {
        match self {
            Self::Directory {
                path,
                children,
                is_album,
                album_cover,
                ..
            } => {
                if !file_path.starts_with(path.as_path()) {
                    return None;
                }
                for child in children {
                    if let Some(album_path) =
                        child.update_album_cover_for_file(file_path, cover.clone())
                    {
                        return Some(album_path);
                    }
                }
                if *is_album {
                    *album_cover = cover;
                    Some(path.clone())
                } else {
                    None
                }
            }
            Self::AudioFile { .. } => None,
        }
    }

    fn append_album_directory_paths(&self, paths: &mut Vec<PathBuf>) {
        if let Self::Directory {
            path,
            children,
            is_album,
            ..
        } = self
        {
            if *is_album {
                paths.push(path.clone());
            }
            for child in children {
                child.append_album_directory_paths(paths);
            }
        }
    }

    fn append_rows(&self, depth: usize, expanded_paths: &[PathBuf], rows: &mut Vec<TreeRow>) {
        match self {
            Self::Directory {
                name,
                path,
                children,
                is_album,
                album_cover,
            } => {
                let expanded = expanded_paths.iter().any(|item| item == path);
                rows.push(TreeRow {
                    name: name.clone(),
                    path: path.clone(),
                    depth,
                    is_directory: true,
                    is_album: *is_album,
                    album_cover: album_cover.clone(),
                    expanded,
                });
                if expanded {
                    for child in children {
                        child.append_rows(depth + 1, expanded_paths, rows);
                    }
                }
            }
            Self::AudioFile { name, path } => rows.push(TreeRow {
                name: name.clone(),
                path: path.clone(),
                depth,
                is_directory: false,
                is_album: false,
                album_cover: None,
                expanded: false,
            }),
        }
    }
}

pub fn audio_paths_between(rows: &[TreeRow], start: &Path, end: &Path) -> Vec<PathBuf> {
    let start_index = rows.iter().position(|row| row.path == start);
    let end_index = rows.iter().position(|row| row.path == end);
    let (Some(start_index), Some(end_index)) = (start_index, end_index) else {
        return Vec::new();
    };
    let (start_index, end_index) = if start_index <= end_index {
        (start_index, end_index)
    } else {
        (end_index, start_index)
    };

    rows[start_index..=end_index]
        .iter()
        .filter(|row| !row.is_directory)
        .map(|row| row.path.clone())
        .collect()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_single_directory_chains() {
        let root = FileTreeNode::directory_with_album_cover(
            PathBuf::from("music"),
            vec![FileTreeNode::directory_with_album_cover(
                PathBuf::from("music/artist"),
                vec![FileTreeNode::directory_with_album_cover(
                    PathBuf::from("music/artist/album"),
                    vec![FileTreeNode::audio(PathBuf::from(
                        "music/artist/album/song.mp3",
                    ))],
                    false,
                    None,
                )],
                false,
                None,
            )],
            false,
            None,
        );

        let FileTreeNode::Directory { name, path, .. } = root else {
            panic!("expected directory")
        };
        assert_eq!(name, "music/artist/album");
        assert_eq!(path, PathBuf::from("music/artist/album"));
    }

    #[test]
    fn updates_the_nearest_album_cover_for_a_saved_file() {
        let first_cover = Arc::new(vec![1, 2, 3]);
        let replacement_cover = Arc::new(vec![4, 5, 6]);
        let song = PathBuf::from("music/artist/album/song.mp3");
        let mut tree = FileTreeNode::directory_with_album_cover(
            PathBuf::from("music"),
            vec![
                FileTreeNode::directory_with_album_cover(
                    PathBuf::from("music/artist"),
                    vec![
                        FileTreeNode::directory_with_album_cover(
                            PathBuf::from("music/artist/album"),
                            vec![FileTreeNode::audio(song.clone())],
                            true,
                            Some(first_cover),
                        ),
                        FileTreeNode::audio(PathBuf::from("music/artist/other.mp3")),
                    ],
                    false,
                    None,
                ),
                FileTreeNode::audio(PathBuf::from("music/loose.mp3")),
            ],
            false,
            None,
        );

        let album_path = tree.update_album_cover_for_file(&song, Some(replacement_cover.clone()));

        assert_eq!(album_path, Some(PathBuf::from("music/artist/album")));
        let album = tree
            .flatten(&[
                PathBuf::from("music/artist"),
                PathBuf::from("music/artist/album"),
            ])
            .into_iter()
            .find(|row| row.path == Path::new("music/artist/album"))
            .expect("album row");
        assert_eq!(album.album_cover, Some(replacement_cover));
    }

    #[test]
    fn preserves_album_cover_in_flattened_rows() {
        let cover = vec![1, 2, 3];
        let tree = FileTreeNode::directory_with_album_cover(
            PathBuf::from("music"),
            vec![
                FileTreeNode::directory_with_album_cover(
                    PathBuf::from("music/album"),
                    vec![FileTreeNode::audio(PathBuf::from("music/album/song.mp3"))],
                    true,
                    Some(Arc::new(cover.clone())),
                ),
                FileTreeNode::audio(PathBuf::from("music/loose-song.mp3")),
            ],
            false,
            None,
        );

        assert_eq!(tree.flatten(&[])[0].album_cover, Some(Arc::new(cover)));
    }

    #[test]
    fn collects_album_paths_from_the_final_tree() {
        let tree = FileTreeNode::directory_with_album_cover(
            PathBuf::from("music"),
            vec![FileTreeNode::directory_with_album_cover(
                PathBuf::from("music/album"),
                vec![FileTreeNode::audio(PathBuf::from("music/album/song.mp3"))],
                true,
                None,
            )],
            false,
            None,
        );

        assert_eq!(
            tree.album_directory_paths(),
            vec![PathBuf::from("music/album")]
        );
    }

    #[test]
    fn selects_only_visible_audio_paths_in_an_inclusive_range() {
        let rows = vec![
            TreeRow {
                name: "Album".into(),
                path: PathBuf::from("music/album"),
                depth: 0,
                is_directory: true,
                is_album: false,
                album_cover: None,
                expanded: true,
            },
            TreeRow {
                name: "first.flac".into(),
                path: PathBuf::from("music/album/first.flac"),
                depth: 1,
                is_directory: false,
                is_album: false,
                album_cover: None,
                expanded: false,
            },
            TreeRow {
                name: "second.flac".into(),
                path: PathBuf::from("music/album/second.flac"),
                depth: 1,
                is_directory: false,
                is_album: false,
                album_cover: None,
                expanded: false,
            },
            TreeRow {
                name: "other.flac".into(),
                path: PathBuf::from("music/other.flac"),
                depth: 0,
                is_directory: false,
                is_album: false,
                album_cover: None,
                expanded: false,
            },
        ];

        assert_eq!(
            audio_paths_between(
                &rows,
                Path::new("music/album/first.flac"),
                Path::new("music/other.flac"),
            ),
            vec![
                PathBuf::from("music/album/first.flac"),
                PathBuf::from("music/album/second.flac"),
                PathBuf::from("music/other.flac"),
            ]
        );
    }

    #[test]
    fn only_expanded_directories_expose_descendants() {
        let root = FileTreeNode::directory_with_album_cover(
            PathBuf::from("music"),
            vec![
                FileTreeNode::directory_with_album_cover(
                    PathBuf::from("music/album"),
                    vec![FileTreeNode::audio(PathBuf::from("music/album/song.mp3"))],
                    false,
                    None,
                ),
                FileTreeNode::audio(PathBuf::from("music/loose-song.mp3")),
            ],
            false,
            None,
        );

        let collapsed_rows = root.flatten(&[]);
        assert_eq!(collapsed_rows.len(), 2);
        assert_eq!(collapsed_rows[0].name, "album");
        assert_eq!(collapsed_rows[0].depth, 0);
        assert_eq!(collapsed_rows[1].name, "loose-song.mp3");
        assert_eq!(root.flatten(&[PathBuf::from("music/album")]).len(), 3);
    }
}
