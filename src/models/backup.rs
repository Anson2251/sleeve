use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupVersion {
    pub timestamp: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}
