use std::{collections::HashSet, path::PathBuf};

use super::AudioFile;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TagField {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Year,
    TrackNumber,
    DiscNumber,
    Genre,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CoverDraft {
    #[default]
    Unavailable,
    Embedded(Vec<u8>),
    External(PathBuf),
    Removed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagDraft {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_number: String,
    pub disc_number: String,
    pub genre: String,
    pub cover: CoverDraft,
}

pub const TAG_FIELDS: [TagField; 8] = [
    TagField::Title,
    TagField::Artist,
    TagField::Album,
    TagField::AlbumArtist,
    TagField::Year,
    TagField::TrackNumber,
    TagField::DiscNumber,
    TagField::Genre,
];

pub fn common_draft(drafts: &[TagDraft]) -> (TagDraft, HashSet<TagField>, bool) {
    let Some(first) = drafts.first() else {
        return (TagDraft::default(), HashSet::new(), false);
    };
    let mut common = first.clone();
    let mut mixed_fields = HashSet::new();
    for field in TAG_FIELDS {
        if drafts
            .iter()
            .skip(1)
            .any(|draft| draft.value(field) != first.value(field))
        {
            common.set(field, String::new());
            mixed_fields.insert(field);
        }
    }
    let covers_mixed = drafts
        .iter()
        .skip(1)
        .any(|draft| draft.cover != first.cover);
    if covers_mixed {
        common.cover = CoverDraft::Unavailable;
    }
    (common, mixed_fields, covers_mixed)
}

impl TagDraft {
    pub fn from_audio_file(file: &AudioFile) -> Self {
        Self {
            title: file.title.clone().unwrap_or_default(),
            artist: file.artist.clone().unwrap_or_default(),
            album: file.album.clone().unwrap_or_default(),
            album_artist: file.album_artist.clone().unwrap_or_default(),
            year: file.year.map(|value| value.to_string()).unwrap_or_default(),
            track_number: file
                .track_number
                .map(|value| value.to_string())
                .unwrap_or_default(),
            disc_number: file
                .disc_number
                .map(|value| value.to_string())
                .unwrap_or_default(),
            genre: file.genre.clone().unwrap_or_default(),
            cover: file
                .embedded_cover
                .clone()
                .map(CoverDraft::Embedded)
                .unwrap_or_default(),
        }
    }

    pub fn validation_error(&self, field: TagField) -> Option<&'static str> {
        let value = match field {
            TagField::Title => &self.title,
            TagField::Artist => &self.artist,
            TagField::Album => &self.album,
            TagField::AlbumArtist => &self.album_artist,
            TagField::Year => &self.year,
            TagField::TrackNumber => &self.track_number,
            TagField::DiscNumber => &self.disc_number,
            TagField::Genre => &self.genre,
        };

        if value.chars().count() > 255 {
            return Some("最长为 255 个字符。");
        }
        match field {
            TagField::Year if !value.is_empty() => match value.parse::<u16>() {
                Ok(year) if (1000..=9999).contains(&year) => None,
                _ => Some("请输入 1000 至 9999 的四位年份。"),
            },
            TagField::TrackNumber | TagField::DiscNumber if !value.is_empty() => {
                match value.parse::<u16>() {
                    Ok(0) => Some("请输入大于 0 的整数。"),
                    Ok(_) => None,
                    Err(_) => Some("请输入 1 至 65535 的整数。"),
                }
            }
            _ => None,
        }
    }

    pub fn is_valid(&self) -> bool {
        [
            TagField::Title,
            TagField::Artist,
            TagField::Album,
            TagField::AlbumArtist,
            TagField::Year,
            TagField::TrackNumber,
            TagField::DiscNumber,
            TagField::Genre,
        ]
        .into_iter()
        .all(|field| self.validation_error(field).is_none())
    }

    pub fn value(&self, field: TagField) -> &str {
        match field {
            TagField::Title => &self.title,
            TagField::Artist => &self.artist,
            TagField::Album => &self.album,
            TagField::AlbumArtist => &self.album_artist,
            TagField::Year => &self.year,
            TagField::TrackNumber => &self.track_number,
            TagField::DiscNumber => &self.disc_number,
            TagField::Genre => &self.genre,
        }
    }

    pub fn with_field(mut self, field: TagField, value: String) -> Self {
        self.set(field, value);
        self
    }

    pub fn set(&mut self, field: TagField, value: String) {
        match field {
            TagField::Title => self.title = value,
            TagField::Artist => self.artist = value,
            TagField::Album => self.album = value,
            TagField::AlbumArtist => self.album_artist = value,
            TagField::Year => self.year = value,
            TagField::TrackNumber => self.track_number = value,
            TagField::DiscNumber => self.disc_number = value,
            TagField::Genre => self.genre = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_year_and_positive_track_number() {
        let draft = TagDraft {
            year: "99".into(),
            track_number: "0".into(),
            ..Default::default()
        };
        assert_eq!(
            draft.validation_error(TagField::Year),
            Some("请输入 1000 至 9999 的四位年份。")
        );
        assert_eq!(
            draft.validation_error(TagField::TrackNumber),
            Some("请输入大于 0 的整数。")
        );
        assert!(!draft.is_valid());
    }

    #[test]
    fn clears_mixed_fields_in_a_common_draft() {
        let first = TagDraft {
            title: "First".into(),
            artist: "Shared".into(),
            ..Default::default()
        };
        let second = TagDraft {
            title: "Second".into(),
            artist: "Shared".into(),
            ..Default::default()
        };

        let (common, mixed, covers_mixed) = common_draft(&[first, second]);

        assert_eq!(common.title, "");
        assert_eq!(common.artist, "Shared");
        assert!(mixed.contains(&TagField::Title));
        assert!(!mixed.contains(&TagField::Artist));
        assert!(!covers_mixed);
    }

    #[test]
    fn clears_a_mixed_cover_from_a_common_draft() {
        let first = TagDraft {
            cover: CoverDraft::Embedded(vec![1, 2, 3]),
            ..Default::default()
        };
        let second = TagDraft {
            cover: CoverDraft::Embedded(vec![4, 5, 6]),
            ..Default::default()
        };

        let (common, _, covers_mixed) = common_draft(&[first, second]);

        assert_eq!(common.cover, CoverDraft::Unavailable);
        assert!(covers_mixed);
    }

    #[test]
    fn cloning_with_a_field_preserves_unrelated_metadata() {
        let draft = TagDraft {
            title: "Original".into(),
            artist: "Artist".into(),
            cover: CoverDraft::Embedded(vec![1, 2, 3]),
            ..Default::default()
        };

        let updated = draft.with_field(TagField::Title, "Updated".into());

        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.artist, "Artist");
        assert_eq!(updated.cover, CoverDraft::Embedded(vec![1, 2, 3]));
    }

    #[test]
    fn setting_a_field_changes_only_that_field() {
        let mut draft = TagDraft {
            title: "Original".into(),
            artist: "Artist".into(),
            ..Default::default()
        };
        draft.set(TagField::Title, "Updated".into());
        assert_eq!(draft.title, "Updated");
        assert_eq!(draft.artist, "Artist");
    }
}
