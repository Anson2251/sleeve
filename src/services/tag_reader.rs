use std::{
    fs,
    path::{Path, PathBuf},
};

use audiotags::Tag;
use symphonia::{
    core::{
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::{MetadataOptions, StandardTagKey, Tag as MetadataTag},
        probe::Hint,
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
    let mut probe_metadata = probe.metadata;
    let mut format = probe.format;
    let probe_encoder = match probe_metadata.get() {
        Some(mut metadata) => metadata
            .skip_to_latest()
            .and_then(|metadata| encoder_from_tags(metadata.tags())),
        None => None,
    };
    let format_encoder = {
        let mut metadata = format.metadata();
        metadata
            .skip_to_latest()
            .and_then(|metadata| encoder_from_tags(metadata.tags()))
    };
    let encoder = probe_encoder.or(format_encoder);
    let track = match format.default_track().or_else(|| format.tracks().first()) {
        Some(track) => track,
        None => return fallback_metadata(path, size),
    };
    let parameters = &track.codec_params;
    let sample_rate = parameters.sample_rate.map(|value| format!("{value} Hz"));
    let channels = parameters.channels.map(|value| value.count()).or_else(|| {
        is_m4a(path)
            .then(|| fs::read(path).ok())
            .flatten()
            .and_then(|bytes| m4a_channels(&bytes))
    });
    let channels = channels.map(format_channels);
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
        codec: encoder.unwrap_or_default(),
        duration,
        bitrate,
        sample_rate,
        channels,
        bits_per_sample,
        file_size: Some(format_file_size(size)),
    }
}

fn is_m4a(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("m4a" | "mp4")
    )
}

fn m4a_channels(bytes: &[u8]) -> Option<usize> {
    bytes.windows(8).enumerate().find_map(|(offset, header)| {
        if header[4..] != *b"stsd" {
            return None;
        }

        let size = usize::try_from(u32::from_be_bytes(header[..4].try_into().ok()?)).ok()?;
        bytes
            .get(offset + 16..offset + size)
            .and_then(m4a_sample_entry_channels)
    })
}

fn m4a_sample_entry_channels(entry: &[u8]) -> Option<usize> {
    entry
        .get(24..26)
        .and_then(|channels| channels.try_into().ok())
        .map(u16::from_be_bytes)
        .filter(|&channels| channels > 0)
        .map(usize::from)
}

fn encoder_from_tags(tags: &[MetadataTag]) -> Option<String> {
    let tag_value = |key| {
        tags.iter()
            .find(|tag| tag.std_key == Some(key))
            .map(|tag| tag.value.to_string())
    };

    tag_value(StandardTagKey::Encoder).or_else(|| tag_value(StandardTagKey::EncoderSettings))
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

    #[test]
    fn prefers_encoder_tag_and_falls_back_to_encoder_settings() {
        let encoder = [MetadataTag::new(
            Some(StandardTagKey::Encoder),
            "ENCODER",
            "Lavf57.71.100".into(),
        )];
        let settings = [MetadataTag::new(
            Some(StandardTagKey::EncoderSettings),
            "ENCODER SETTINGS",
            "-compression_level 8".into(),
        )];

        assert_eq!(encoder_from_tags(&encoder), Some("Lavf57.71.100".into()));
        assert_eq!(
            encoder_from_tags(&settings),
            Some("-compression_level 8".into())
        );
    }

    #[test]
    fn reads_m4a_channel_count_from_audio_sample_entry() {
        let mut entry = Vec::new();
        entry.extend_from_slice(&36u32.to_be_bytes());
        entry.extend_from_slice(b"mp4a");
        entry.extend_from_slice(&[0; 6]);
        entry.extend_from_slice(&1u16.to_be_bytes());
        entry.extend_from_slice(&[0; 8]);
        entry.extend_from_slice(&2u16.to_be_bytes());
        entry.extend_from_slice(&16u16.to_be_bytes());
        entry.extend_from_slice(&[0; 4]);
        entry.extend_from_slice(&(44_100u32 << 16).to_be_bytes());

        assert_eq!(m4a_sample_entry_channels(&entry), Some(2));

        let mut stsd = Vec::new();
        stsd.extend_from_slice(&52u32.to_be_bytes());
        stsd.extend_from_slice(b"stsd");
        stsd.extend_from_slice(&[0; 4]);
        stsd.extend_from_slice(&1u32.to_be_bytes());
        stsd.extend_from_slice(&entry);

        assert_eq!(m4a_channels(&stsd), Some(2));
    }
}
