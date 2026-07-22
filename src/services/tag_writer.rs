use std::{io::Cursor, path::Path};

use audiotags::{AudioTagEdit, MimeType, Picture, Tag};

use crate::models::{CoverDraft, TagDraft};

pub fn write_draft(path: &Path, draft: &TagDraft) -> Result<(), String> {
    let mut tag = Tag::new()
        .read_from_path(path)
        .map_err(|error| crate::tf!("error.write_read_tag", "error" => &error.to_string()))?;

    set_or_remove(
        &mut *tag,
        &draft.title,
        |tag, value| tag.set_title(value),
        |tag| tag.remove_title(),
    );
    set_or_remove(
        &mut *tag,
        &draft.artist,
        |tag, value| tag.set_artist(value),
        |tag| tag.remove_artist(),
    );
    set_or_remove(
        &mut *tag,
        &draft.album,
        |tag, value| tag.set_album_title(value),
        |tag| tag.remove_album_title(),
    );
    set_or_remove(
        &mut *tag,
        &draft.album_artist,
        |tag, value| tag.set_album_artist(value),
        |tag| tag.remove_album_artist(),
    );
    set_or_remove(
        &mut *tag,
        &draft.genre,
        |tag, value| tag.set_genre(value),
        |tag| tag.remove_genre(),
    );
    set_year(&mut *tag, &draft.year)?;
    set_number(
        &mut *tag,
        &draft.track_number,
        |tag, value| tag.set_track_number(value),
        |tag| tag.remove_track_number(),
    )?;
    set_number(
        &mut *tag,
        &draft.disc_number,
        |tag, value| tag.set_disc_number(value),
        |tag| tag.remove_disc_number(),
    )?;
    apply_cover(&mut *tag, &draft.cover)?;

    tag.write_to_path(&path.to_string_lossy())
        .map_err(|error| crate::tf!("error.write_tag", "error" => &error.to_string()))
}

fn set_or_remove<T: AudioTagEdit + ?Sized>(
    tag: &mut T,
    value: &str,
    set: impl FnOnce(&mut T, &str),
    remove: impl FnOnce(&mut T),
) {
    if value.is_empty() {
        remove(tag);
    } else {
        set(tag, value);
    }
}

fn set_year<T: AudioTagEdit + ?Sized>(tag: &mut T, value: &str) -> Result<(), String> {
    if value.is_empty() {
        tag.remove_year();
        return Ok(());
    }
    let year = value
        .parse::<i32>()
        .map_err(|_| crate::t!("error.invalid_year"))?;
    tag.set_year(year);
    Ok(())
}

fn set_number<T, F, R>(tag: &mut T, value: &str, set: F, remove: R) -> Result<(), String>
where
    T: AudioTagEdit + ?Sized,
    F: FnOnce(&mut T, u16),
    R: FnOnce(&mut T),
{
    if value.is_empty() {
        remove(tag);
        return Ok(());
    }
    let value = value
        .parse::<u16>()
        .map_err(|_| crate::t!("error.invalid_number"))?;
    set(tag, value);
    Ok(())
}

fn apply_cover<T: AudioTagEdit + ?Sized>(tag: &mut T, cover: &CoverDraft) -> Result<(), String> {
    match cover {
        CoverDraft::Unavailable => {}
        CoverDraft::Removed => tag.remove_album_cover(),
        CoverDraft::Embedded(bytes) | CoverDraft::Converted(bytes) => {
            tag.set_album_cover(Picture::new(bytes, infer_mime_type(bytes)?))
        }
    }
    Ok(())
}

pub fn normalize_cover_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let format = image::guess_format(bytes)
        .map_err(|error| crate::tf!("error.cover_type", "error" => &error.to_string()))?;
    if matches!(
        format,
        image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::Gif
    ) {
        return Ok(bytes.to_vec());
    }

    let image = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| crate::tf!("error.cover_type", "error" => &error.to_string()))?;
    let mut jpeg = Cursor::new(Vec::new());
    image
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .map_err(|error| crate::tf!("error.cover_type", "error" => &error.to_string()))?;
    Ok(jpeg.into_inner())
}

fn infer_mime_type(bytes: &[u8]) -> Result<MimeType, String> {
    image::guess_format(bytes)
        .map_err(|error| crate::tf!("error.cover_type", "error" => &error.to_string()))
        .and_then(|format| match format {
            image::ImageFormat::Jpeg => Ok(MimeType::Jpeg),
            image::ImageFormat::Png => Ok(MimeType::Png),
            image::ImageFormat::Gif => Ok(MimeType::Gif),
            image::ImageFormat::Bmp => Ok(MimeType::Bmp),
            image::ImageFormat::Tiff => Ok(MimeType::Tiff),
            _ => Err(crate::t!("error.cover_format")),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_non_native_cover_formats_to_jpeg() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([12, 34, 56, 255]),
        ));

        for format in [
            image::ImageFormat::WebP,
            image::ImageFormat::Bmp,
            image::ImageFormat::Tiff,
        ] {
            let mut source = std::io::Cursor::new(Vec::new());
            image.write_to(&mut source, format).unwrap();

            let converted = normalize_cover_bytes(&source.into_inner()).unwrap();

            assert_eq!(
                image::guess_format(&converted).unwrap(),
                image::ImageFormat::Jpeg
            );
            assert!(matches!(infer_mime_type(&converted), Ok(MimeType::Jpeg)));
        }
    }
}
