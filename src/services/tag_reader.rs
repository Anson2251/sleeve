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
    let encoder = probe_encoder
        .or(format_encoder)
        .or_else(|| flac_vendor_from_file(path));
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

fn flac_vendor_from_file(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| extension.eq_ignore_ascii_case("flac"))
        .and_then(|_| fs::read(path).ok())
        .and_then(|bytes| flac_vendor_string(&bytes))
}

fn flac_vendor_string(bytes: &[u8]) -> Option<String> {
    let mut offset = flac_marker_offset(bytes)? + 4;

    loop {
        let header = bytes.get(offset..offset + 4)?;
        let is_last = header[0] & 0x80 != 0;
        let block_type = header[0] & 0x7f;
        let block_length =
            usize::try_from(u32::from_be_bytes([0, header[1], header[2], header[3]])).ok()?;
        offset += 4;
        let block = bytes.get(offset..offset + block_length)?;

        if block_type == 4 {
            let vendor_length =
                usize::try_from(u32::from_le_bytes(block.get(..4)?.try_into().ok()?)).ok()?;
            let vendor = std::str::from_utf8(block.get(4..4 + vendor_length)?)
                .ok()?
                .trim();
            let vendor = vendor.strip_prefix("reference ").unwrap_or(vendor);
            return (!vendor.is_empty()).then(|| vendor.to_owned());
        }
        if is_last {
            return None;
        }
        offset += block_length;
    }
}

fn flac_marker_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.starts_with(b"fLaC") {
        return Some(0);
    }
    let header = bytes.get(..10)?;
    if &header[..3] != b"ID3" {
        return None;
    }

    let tag_size = header[6..10].iter().try_fold(0usize, |size, byte| {
        (*byte <= 0x7f).then_some((size << 7) | usize::from(*byte))
    })?;
    let footer_size = usize::from(header[5] & 0x10 != 0) * 10;
    let marker_offset = 10usize.checked_add(tag_size)?.checked_add(footer_size)?;
    bytes
        .get(marker_offset..marker_offset + 4)
        .filter(|marker| *marker == b"fLaC")
        .map(|_| marker_offset)
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
    fn reads_flac_vorbis_comment_vendor_as_encoder() {
        let vendor = b"reference libFLAC 1.2.1 20070917";
        let mut comment = Vec::new();
        comment.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        comment.extend_from_slice(vendor);
        comment.extend_from_slice(&0u32.to_le_bytes());

        let mut flac = b"fLaC".to_vec();
        flac.push(0x80 | 4);
        flac.extend_from_slice(&(comment.len() as u32).to_be_bytes()[1..]);
        flac.extend_from_slice(&comment);

        assert_eq!(
            flac_vendor_string(&flac),
            Some("libFLAC 1.2.1 20070917".into())
        );
    }

    #[test]
    fn reads_flac_vendor_after_id3v2_prefix() {
        let vendor = b"reference libFLAC 1.3.1 20141125";
        let mut comment = Vec::new();
        comment.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        comment.extend_from_slice(vendor);
        comment.extend_from_slice(&0u32.to_le_bytes());

        let mut flac = b"fLaC".to_vec();
        flac.push(0x80 | 4);
        flac.extend_from_slice(&(comment.len() as u32).to_be_bytes()[1..]);
        flac.extend_from_slice(&comment);

        let mut id3 = b"ID3\x03\0\0\0\0\0\x05".to_vec();
        id3.extend_from_slice(b"abcde");
        id3.extend_from_slice(&flac);

        assert_eq!(
            flac_vendor_string(&id3),
            Some("libFLAC 1.3.1 20141125".into())
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
