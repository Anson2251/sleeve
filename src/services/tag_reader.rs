use std::{
    fs,
    path::{Path, PathBuf},
};

use audiotags::Tag;
use symphonia::{
    core::{
        codecs::CODEC_TYPE_NULL, formats::FormatOptions, io::MediaSourceStream,
        meta::MetadataOptions, probe::Hint,
    },
    default::get_probe,
};

use crate::models::{AudioFile, AudioMetadata};

pub fn read_audio_file(path: PathBuf, root: PathBuf) -> Result<AudioFile, String> {
    let tag = Tag::new()
        .read_from_path(&path)
        .map_err(|error| format!("无法读取标签：{error}"))?;
    let filesystem_metadata =
        fs::metadata(&path).map_err(|error| format!("无法读取文件信息：{error}"))?;
    let (track_number, _) = tag.track();
    let (disc_number, _) = tag.disc();

    Ok(AudioFile {
        relative_path: path.strip_prefix(&root).unwrap_or(&path).to_owned(),
        path: path.clone(),
        title: tag.title().map(str::to_owned),
        artist: tag.artist().map(str::to_owned),
        album: tag.album_title().map(str::to_owned),
        album_artist: tag.album_artist().map(str::to_owned),
        year: tag.year(),
        track_number,
        disc_number,
        genre: tag.genre().map(str::to_owned),
        embedded_cover: tag.album_cover().map(|picture| picture.data.to_vec()),
        metadata: inspect_media(&path, filesystem_metadata.len()),
    })
}

fn inspect_media(path: &Path, size: u64) -> AudioMetadata {
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let source = match fs::File::open(path) {
        Ok(file) => MediaSourceStream::new(Box::new(file), Default::default()),
        Err(_) => return fallback_metadata(path, size),
    };
    let probe = match get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) {
        Ok(probe) => probe,
        Err(_) => return fallback_metadata(path, size),
    };
    let format = probe.format;
    let track = match format.default_track().or_else(|| format.tracks().first()) {
        Some(track) => track,
        None => return fallback_metadata(path, size),
    };
    let parameters = &track.codec_params;
    let sample_rate = parameters.sample_rate.map(|value| format!("{value} Hz"));
    let channels = parameters
        .channels
        .map(|value| format_channels(value.count()));
    let bits_per_sample = parameters
        .bits_per_sample
        .map(|value| format!("{value}-bit"));
    let duration = parameters
        .n_frames
        .zip(parameters.sample_rate)
        .map(|(frames, rate)| format_duration(frames as f64 / rate as f64));
    let bitrate = duration
        .as_ref()
        .and_then(|_| parameters.n_frames.zip(parameters.sample_rate))
        .and_then(|(frames, rate)| {
            if frames == 0 || rate == 0 {
                None
            } else {
                let seconds = frames as f64 / rate as f64;
                Some(format!(
                    "{:.0} kbps",
                    (size as f64 * 8.0 / seconds) / 1000.0
                ))
            }
        });

    AudioMetadata {
        container: container_name(path),
        codec: if parameters.codec == CODEC_TYPE_NULL {
            "未知".into()
        } else {
            format!("{:?}", parameters.codec)
        },
        duration,
        bitrate,
        sample_rate,
        channels,
        bits_per_sample,
        file_size: Some(format_file_size(size)),
    }
}

fn fallback_metadata(path: &Path, size: u64) -> AudioMetadata {
    AudioMetadata {
        container: container_name(path),
        file_size: Some(format_file_size(size)),
        ..Default::default()
    }
}

fn container_name(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("未知")
        .to_ascii_uppercase()
}

fn format_channels(count: usize) -> String {
    match count {
        1 => "单声道 (1)".into(),
        2 => "立体声 (2)".into(),
        count => format!("{count} 声道"),
    }
}

fn format_duration(seconds: f64) -> String {
    let seconds = seconds.round() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn format_file_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const KIB: u64 = 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_duration_size_and_channels() {
        assert_eq!(format_duration(61.2), "1:01");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_channels(2), "立体声 (2)");
    }
}
