use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioMetadata {
    pub container: String,
    pub codec: String,
    pub duration: Option<String>,
    pub bitrate: Option<String>,
    pub sample_rate: Option<String>,
    pub channels: Option<String>,
    pub bits_per_sample: Option<String>,
    pub file_size: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct AudioFile {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<u16>,
    pub disc_number: Option<u16>,
    pub genre: Option<String>,
    pub embedded_cover: Option<Vec<u8>>,
    pub metadata: AudioMetadata,
}
