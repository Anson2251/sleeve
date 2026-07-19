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
        self.append_rows(0, expanded_paths, &mut rows);
        rows
    }

    pub fn album_directory_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        self.append_album_directory_paths(&mut paths);
        paths
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
    fn preserves_album_cover_in_flattened_rows() {
        let cover = vec![1, 2, 3];
        let tree = FileTreeNode::directory_with_album_cover(
            PathBuf::from("album"),
            vec![FileTreeNode::audio(PathBuf::from("album/song.mp3"))],
            true,
            Some(Arc::new(cover.clone())),
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
    fn only_expanded_directories_expose_descendants() {
        let root = FileTreeNode::directory_with_album_cover(
            PathBuf::from("music"),
            vec![FileTreeNode::directory_with_album_cover(
                PathBuf::from("music/album"),
                vec![FileTreeNode::audio(PathBuf::from("music/album/song.mp3"))],
                false,
                None,
            )],
            false,
            None,
        );

        assert_eq!(root.flatten(&[]).len(), 1);
        assert_eq!(root.flatten(&[PathBuf::from("music/album")]).len(), 2);
    }
}
