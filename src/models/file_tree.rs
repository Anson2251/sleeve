use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileTreeNode {
    Directory {
        name: String,
        path: PathBuf,
        children: Vec<FileTreeNode>,
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
    pub expanded: bool,
}

impl FileTreeNode {
    pub fn directory(path: PathBuf, children: Vec<Self>) -> Self {
        Self::Directory {
            name: display_name(&path),
            path,
            children,
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

    fn append_rows(&self, depth: usize, expanded_paths: &[PathBuf], rows: &mut Vec<TreeRow>) {
        match self {
            Self::Directory {
                name,
                path,
                children,
            } => {
                let expanded = expanded_paths.iter().any(|item| item == path);
                rows.push(TreeRow {
                    name: name.clone(),
                    path: path.clone(),
                    depth,
                    is_directory: true,
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
    fn only_expanded_directories_expose_descendants() {
        let root = FileTreeNode::directory(
            PathBuf::from("music"),
            vec![FileTreeNode::directory(
                PathBuf::from("music/album"),
                vec![FileTreeNode::audio(PathBuf::from("music/album/song.mp3"))],
            )],
        );

        assert_eq!(root.flatten(&[]).len(), 1);
        assert_eq!(root.flatten(&[PathBuf::from("music")]).len(), 2);
        assert_eq!(
            root.flatten(&[PathBuf::from("music"), PathBuf::from("music/album")])
                .len(),
            3
        );
    }
}
