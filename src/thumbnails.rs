// SPDX-License-Identifier: GPL-3.0-only

//! Previews of the pictures in the Downloads folder.
//!
//! A picture is decoded on a blocking thread, turned the right way up (phone photos store their
//! rotation in EXIF), scaled down to [`SIZE`] and kept in memory. Previews are keyed by path,
//! size and modification time, so each picture is decoded once while the applet runs, and again
//! only if the file changes.

use cosmic::iced::widget::image::Handle;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, MutexGuard, PoisonError},
    time::SystemTime,
};

/// Longest side of a preview, in pixels. The fan draws it 32 logical px across, so this stays
/// sharp up to 3x scaling.
pub const SIZE: u32 = 96;
/// Bigger files keep their file-type icon instead of being decoded.
const MAX_FILE_SIZE: u64 = 100 * 1000 * 1000;

/// Identifies one version of a file, so an edited or replaced picture gets a new preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub len: u64,
    pub modified: SystemTime,
}

/// A decoded preview. Cloning keeps the same image handle, so it's uploaded to the GPU once.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub handle: Handle,
    pub width: u32,
    pub height: u32,
}

impl Thumbnail {
    /// Width and height that fit inside a `side` px square, keeping the picture's shape.
    #[allow(clippy::cast_precision_loss)]
    pub fn fit(&self, side: f32) -> (f32, f32) {
        let longest = self.width.max(self.height).max(1) as f32;
        (
            side * self.width as f32 / longest,
            side * self.height as f32 / longest,
        )
    }
}

struct Cached {
    stamp: Stamp,
    /// `None` when the file couldn't be decoded; it isn't tried again until it changes.
    thumbnail: Option<Thumbnail>,
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, Cached>>> = LazyLock::new(Mutex::default);

fn cache() -> MutexGuard<'static, HashMap<PathBuf, Cached>> {
    CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `path` looks like a picture that can be decoded here, going by its extension.
pub fn supported(path: &Path) -> bool {
    ImageFormat::from_path(path).is_ok_and(|format| format.reading_enabled())
}

/// Whether `path` is an SVG. Those are drawn as vectors, so they need no decoding.
pub fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

/// What's known about the preview of one version of a file.
#[derive(Debug, Clone)]
pub enum Lookup {
    Ready(Thumbnail),
    /// The file couldn't be decoded.
    Failed,
    /// Not made yet.
    Missing,
}

/// The preview of this version of `path`, if it has been made already.
pub fn cached(path: &Path, stamp: Stamp) -> Lookup {
    match cache().get(path).filter(|cached| cached.stamp == stamp) {
        Some(Cached {
            thumbnail: Some(thumbnail),
            ..
        }) => Lookup::Ready(thumbnail.clone()),
        Some(_) => Lookup::Failed,
        None => Lookup::Missing,
    }
}

/// Decodes `path` into a preview and remembers it. Blocks while decoding.
pub fn load(path: &Path, stamp: Stamp) -> Option<Thumbnail> {
    let thumbnail = if stamp.len <= MAX_FILE_SIZE {
        decode(path)
            .inspect_err(|err| tracing::debug!("no preview for {}: {err}", path.display()))
            .ok()
    } else {
        None
    };

    cache().insert(
        path.to_owned(),
        Cached {
            stamp,
            thumbnail: thumbnail.clone(),
        },
    );
    thumbnail
}

/// Forgets the previews of files that `keep` turns down, so the cache stays the size of the stack.
pub fn retain(keep: impl Fn(&Path) -> bool) {
    cache().retain(|path, _| keep(path));
}

fn decode(path: &Path) -> image::ImageResult<Thumbnail> {
    // The contents decide the format, so a PNG saved as `.jpg` still opens.
    let mut decoder = ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut picture = DynamicImage::from_decoder(decoder)?;
    picture.apply_orientation(orientation);

    // Small pictures are left at their own size rather than blown up.
    let picture = if picture.width().max(picture.height()) > SIZE {
        picture.thumbnail(SIZE, SIZE)
    } else {
        picture
    };

    let rgba = picture.into_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(Thumbnail {
        handle: Handle::from_rgba(width, height, rgba.into_raw()),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::fs;

    #[test]
    fn pictures_are_recognised_by_extension() {
        assert!(supported(Path::new("photo.JPG")));
        assert!(supported(Path::new("shot.png")));
        assert!(supported(Path::new("pic.webp")));
        assert!(!supported(Path::new("notes.txt")));
        assert!(!supported(Path::new("logo.svg")));
        assert!(is_svg(Path::new("logo.SVG")));
    }

    #[test]
    fn previews_keep_their_shape_and_are_cached() {
        let dir = std::env::temp_dir().join(format!("ds-thumb-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.png");
        ImageBuffer::from_pixel(400, 200, Rgba([200u8, 40, 40, 255]))
            .save(&path)
            .unwrap();

        let meta = fs::metadata(&path).unwrap();
        let stamp = Stamp {
            len: meta.len(),
            modified: meta.modified().unwrap(),
        };
        assert!(matches!(cached(&path, stamp), Lookup::Missing));

        let thumb = load(&path, stamp).unwrap();
        assert_eq!((thumb.width, thumb.height), (SIZE, SIZE / 2));
        assert_eq!(thumb.fit(32.0), (32.0, 16.0));
        assert!(matches!(cached(&path, stamp), Lookup::Ready(_)));

        // A changed file isn't served the old preview.
        let newer = Stamp {
            len: stamp.len + 1,
            ..stamp
        };
        assert!(matches!(cached(&path, newer), Lookup::Missing));

        // A file that isn't really a picture is remembered as failed.
        let broken = dir.join("broken.png");
        fs::write(&broken, b"not a png").unwrap();
        let broken_stamp = Stamp { len: 9, ..stamp };
        assert!(load(&broken, broken_stamp).is_none());
        assert!(matches!(cached(&broken, broken_stamp), Lookup::Failed));

        fs::remove_dir_all(&dir).unwrap();
    }
}
