use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    rc::Rc,
    time::{Duration, Instant},
};

use image::GenericImageView;
use libblur::{AnisotropicRadius, BlurImageMut, FastBlurChannels, ThreadingPolicy, stack_blur};
use relm4::gtk::{
    self,
    gdk::prelude::GdkCairoContextExt,
    prelude::{CastNone, WidgetExt, WidgetExtManual},
};

use crate::models::CoverDraft;

const EDITOR_COVER_TRANSITION_DURATION: Duration = Duration::from_millis(300);
const BLURRED_COVER_CACHE_CAPACITY: usize = 16;
const COVER_PREVIEW_MAX_SIZE: i32 = 520;
const EDITOR_COVER_BACKGROUND_SIZE: u32 = 500;
const EDITOR_COVER_BLUR_RADIUS: u32 = 18;

#[derive(Debug, Clone)]
pub(super) struct BlurredCover {
    hash: u64,
    pub(super) pixbuf: gdk_pixbuf::Pixbuf,
}

#[derive(Debug, Default)]
pub(super) struct BlurredCoverCache {
    entries: HashMap<u64, gdk_pixbuf::Pixbuf>,
    usage_order: VecDeque<u64>,
}

impl BlurredCoverCache {
    fn get(&mut self, hash: u64) -> Option<BlurredCover> {
        let pixbuf = self.entries.get(&hash)?.clone();
        self.touch(hash);
        Some(BlurredCover { hash, pixbuf })
    }

    fn insert(&mut self, cover: BlurredCover) {
        self.entries.insert(cover.hash, cover.pixbuf);
        self.touch(cover.hash);
        while self.entries.len() > BLURRED_COVER_CACHE_CAPACITY {
            if let Some(hash) = self.usage_order.pop_front() {
                self.entries.remove(&hash);
            }
        }
    }

    fn touch(&mut self, hash: u64) {
        self.usage_order.retain(|cached_hash| *cached_hash != hash);
        self.usage_order.push_back(hash);
    }
}

#[derive(Debug, Default)]
pub(super) struct EditorCoverTransition {
    pub(super) previous: Option<BlurredCover>,
    pub(super) current: Option<BlurredCover>,
    started_at: Option<Instant>,
}

pub(super) fn update_cover_background(
    overlay: &gtk::Overlay,
    editor_cover: &Rc<RefCell<EditorCoverTransition>>,
    cache: &RefCell<BlurredCoverCache>,
    cover: &CoverDraft,
) {
    let Some(background) = overlay.child().and_downcast::<gtk::DrawingArea>() else {
        return;
    };
    let next_cover = cached_blurred_cover(cache, cover);
    let mut transition = editor_cover.borrow_mut();
    if transition.current.as_ref().map(|cover| cover.hash)
        == next_cover.as_ref().map(|cover| cover.hash)
    {
        return;
    }

    transition.previous = transition.current.take();
    transition.current = next_cover;
    transition.started_at =
        (transition.previous.is_some() || transition.current.is_some()).then(Instant::now);
    let animate = transition.started_at.is_some();
    drop(transition);

    background.queue_draw();
    if animate {
        let transition = editor_cover.clone();
        background.add_tick_callback(move |widget, _| {
            widget.queue_draw();
            if transition_progress(&transition.borrow()) >= 1.0 {
                let mut transition = transition.borrow_mut();
                transition.previous = None;
                transition.started_at = None;
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}

pub(super) fn transition_progress(transition: &EditorCoverTransition) -> f64 {
    let Some(started_at) = transition.started_at else {
        return 1.0;
    };
    let progress = (started_at.elapsed().as_secs_f64()
        / EDITOR_COVER_TRANSITION_DURATION.as_secs_f64())
    .clamp(0.0, 1.0);
    1.0 - (1.0 - progress).powi(3)
}

pub(super) fn draw_cover(
    context: &gtk::cairo::Context,
    pixbuf: &gdk_pixbuf::Pixbuf,
    width: i32,
    height: i32,
    opacity: f64,
) {
    if opacity <= 0.0 {
        return;
    }
    let scale = (width as f64 / pixbuf.width() as f64).max(height as f64 / pixbuf.height() as f64);
    let scaled_width = pixbuf.width() as f64 * scale;
    let scaled_height = pixbuf.height() as f64 * scale;
    let x = (width as f64 - scaled_width) / 2.0;
    let y = (height as f64 - scaled_height) / 2.0;

    let _ = context.save();
    context.rectangle(0.0, 0.0, width as f64, height as f64);
    context.clip();
    context.scale(scale, scale);
    context.set_source_pixbuf(pixbuf, x / scale, y / scale);
    let _ = context.paint_with_alpha(opacity);
    let _ = context.restore();
}

pub(super) fn update_cover(picture: &gtk::Picture, cover: &CoverDraft) -> String {
    picture.set_filename(None::<&str>);
    picture.set_pixbuf(None::<&gdk_pixbuf::Pixbuf>);

    let byte_size = match cover {
        CoverDraft::External(path) => std::fs::metadata(path).ok().map(|metadata| metadata.len()),
        CoverDraft::Embedded(bytes) => Some(bytes.len() as u64),
        CoverDraft::Unavailable | CoverDraft::Removed => None,
    };

    match cover_pixbuf(cover) {
        Some(pixbuf) => {
            let dimensions = format!("{} × {} px", pixbuf.width(), pixbuf.height());
            let size = byte_size
                .map(format_byte_size)
                .unwrap_or_else(|| crate::t!("cover.unknown_size"));
            picture.set_pixbuf(Some(&scale_cover_preview(&pixbuf)));
            format!("{dimensions} · {size}")
        }
        None => crate::t!("cover.no_image"),
    }
}

fn cached_blurred_cover(
    cache: &RefCell<BlurredCoverCache>,
    cover: &CoverDraft,
) -> Option<BlurredCover> {
    let hash = cover_hash(cover)?;
    if let Some(cover) = cache.borrow_mut().get(hash) {
        return Some(cover);
    }

    let pixbuf = blurred_cover_pixbuf(cover)?;
    let cover = BlurredCover { hash, pixbuf };
    cache.borrow_mut().insert(cover.clone());
    Some(cover)
}

fn cover_hash(cover: &CoverDraft) -> Option<u64> {
    let bytes = match cover {
        CoverDraft::External(path) => std::fs::read(path).ok()?,
        CoverDraft::Embedded(bytes) => bytes.clone(),
        CoverDraft::Unavailable | CoverDraft::Removed => return None,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}

fn blurred_cover_pixbuf(cover: &CoverDraft) -> Option<gdk_pixbuf::Pixbuf> {
    let rgb = decoded_cover_image(cover)?
        .resize_to_fill(
            EDITOR_COVER_BACKGROUND_SIZE,
            EDITOR_COVER_BACKGROUND_SIZE,
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8();
    let (width, height) = rgb.dimensions();
    let mut pixels = rgb.into_raw();
    let mut blur_image =
        BlurImageMut::borrow(&mut pixels, width, height, FastBlurChannels::Channels3);
    stack_blur(
        &mut blur_image,
        AnisotropicRadius::new(EDITOR_COVER_BLUR_RADIUS),
        ThreadingPolicy::Adaptive,
    )
    .ok()?;

    Some(gdk_pixbuf::Pixbuf::from_bytes(
        &glib::Bytes::from_owned(pixels),
        gdk_pixbuf::Colorspace::Rgb,
        false,
        8,
        width as i32,
        height as i32,
        (width * 3) as i32,
    ))
}

fn cover_pixbuf(cover: &CoverDraft) -> Option<gdk_pixbuf::Pixbuf> {
    let image = decoded_cover_image(cover)?;
    let (width, height) = image.dimensions();
    let pixels = image.to_rgba8().into_raw();

    Some(gdk_pixbuf::Pixbuf::from_bytes(
        &glib::Bytes::from_owned(pixels),
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        width as i32,
        height as i32,
        (width * 4) as i32,
    ))
}

fn decoded_cover_image(cover: &CoverDraft) -> Option<image::DynamicImage> {
    match cover {
        CoverDraft::External(path) => image::ImageReader::open(path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok(),
        CoverDraft::Embedded(bytes) => image::load_from_memory(bytes).ok(),
        CoverDraft::Unavailable | CoverDraft::Removed => None,
    }
}

fn format_byte_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const KIB: u64 = 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn scale_cover_preview(pixbuf: &gdk_pixbuf::Pixbuf) -> gdk_pixbuf::Pixbuf {
    let width = pixbuf.width();
    let height = pixbuf.height();
    let scale = (COVER_PREVIEW_MAX_SIZE as f64 / width as f64)
        .min(COVER_PREVIEW_MAX_SIZE as f64 / height as f64)
        .min(1.0);

    if scale == 1.0 {
        return pixbuf.clone();
    }

    let scaled_width = (width as f64 * scale).round() as i32;
    let scaled_height = (height as f64 * scale).round() as i32;
    pixbuf
        .scale_simple(
            scaled_width.max(1),
            scaled_height.max(1),
            gdk_pixbuf::InterpType::Bilinear,
        )
        .unwrap_or_else(|| pixbuf.clone())
}
