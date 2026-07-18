use std::{fs, path::Path};

use audiotags::{AudioTagEdit, MimeType, Picture, Tag};

use crate::models::{CoverDraft, TagDraft};

pub fn write_draft(path: &Path, draft: &TagDraft) -> Result<(), String> {
    let mut tag = Tag::new()
        .read_from_path(path)
        .map_err(|error| format!("无法读取待保存标签：{error}"))?;

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
        .map_err(|error| format!("无法写入标签：{error}"))
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
        .map_err(|_| "年份无效，无法保存。".to_string())?;
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
        .map_err(|_| "数值字段无效，无法保存。".to_string())?;
    set(tag, value);
    Ok(())
}

fn apply_cover<T: AudioTagEdit + ?Sized>(tag: &mut T, cover: &CoverDraft) -> Result<(), String> {
    match cover {
        CoverDraft::Unavailable => {}
        CoverDraft::Removed => tag.remove_album_cover(),
        CoverDraft::Embedded(bytes) => {
            tag.set_album_cover(Picture::new(bytes, infer_mime_type(bytes)?))
        }
        CoverDraft::External(path) => {
            let bytes = fs::read(path).map_err(|error| format!("无法读取封面图片：{error}"))?;
            let mime_type = infer_mime_type(&bytes)?;
            tag.set_album_cover(Picture::new(&bytes, mime_type));
        }
    }
    Ok(())
}

fn infer_mime_type(bytes: &[u8]) -> Result<MimeType, String> {
    image::guess_format(bytes)
        .map_err(|error| format!("无法识别封面图片类型：{error}"))
        .and_then(|format| match format {
            image::ImageFormat::Jpeg => Ok(MimeType::Jpeg),
            image::ImageFormat::Png => Ok(MimeType::Png),
            image::ImageFormat::Gif => Ok(MimeType::Gif),
            image::ImageFormat::Bmp => Ok(MimeType::Bmp),
            image::ImageFormat::Tiff => Ok(MimeType::Tiff),
            _ => Err("封面格式必须为 JPEG、PNG、GIF、BMP 或 TIFF。".into()),
        })
}
