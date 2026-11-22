use crate::{DevicePixels, Pixels, Result, SharedString, Size, size};
use smallvec::SmallVec;

use image::{Delay, Frame};
use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

/// A source of assets for this app to use.
pub trait AssetSource: 'static + Send + Sync {
    /// Load the given asset from the source path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;

    /// List the assets at the given path.
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}

impl AssetSource for () {
    fn load(&self, _path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(None)
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

/// A unique identifier for the image cache
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ImageId(pub usize);

#[derive(PartialEq, Eq, Hash, Clone)]
pub(crate) struct RenderImageParams {
    pub(crate) image_id: ImageId,
    pub(crate) frame_index: usize,
}

/// A cached and processed image, in BGRA format
pub struct RenderImage {
    /// The ID associated with this image
    pub id: ImageId,
    /// The scale factor of this image on render.
    pub(crate) scale_factor: f32,
    data: RenderImageData,
}

enum EphemeralPixels {
    Owned(Vec<u8>),
    Shared(Arc<Vec<u8>>),
}

impl EphemeralPixels {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(pixels) => pixels.as_slice(),
            Self::Shared(pixels) => pixels.as_slice(),
        }
    }

    fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Owned(pixels) => pixels,
            Self::Shared(pixels) => Arc::try_unwrap(pixels).unwrap_or_else(|p| (*p).clone()),
        }
    }
}

enum RenderImageData {
    Frames(SmallVec<[Frame; 1]>),
    Shared {
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    },
    /// Still BGRA image whose CPU pixels are moved out and freed on first GPU upload.
    Ephemeral {
        width: u32,
        height: u32,
        pixels: parking_lot::Mutex<Option<EphemeralPixels>>,
    },
}

impl PartialEq for RenderImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RenderImage {}

impl RenderImage {
    /// Create a new image from the given data.
    pub fn new(data: impl Into<SmallVec<[Frame; 1]>>) -> Self {
        Self::with_data(RenderImageData::Frames(data.into()))
    }

    /// Create a still image sharing an existing straight-BGRA pixel buffer.
    /// Invalid dimensions or a buffer whose length does not match are rejected.
    pub fn from_shared_bgra(width: u32, height: u32, pixels: Arc<Vec<u8>>) -> Option<Self> {
        let length = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || pixels.len() != length {
            return None;
        }
        Some(Self::with_data(RenderImageData::Shared {
            width,
            height,
            pixels,
        }))
    }

    /// Create a still straight-BGRA image whose CPU pixels are freed as soon as
    /// they have been uploaded into the GPU sprite atlas (on platforms without
    /// D3D11 device-lost atlas invalidation).
    pub fn from_ephemeral_bgra(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        let length = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || pixels.len() != length {
            return None;
        }
        #[cfg(target_os = "windows")]
        {
            Some(Self::with_data(RenderImageData::Shared {
                width,
                height,
                pixels: Arc::new(pixels),
            }))
        }
        #[cfg(not(target_os = "windows"))]
        {
            Some(Self::with_data(RenderImageData::Ephemeral {
                width,
                height,
                pixels: parking_lot::Mutex::new(Some(EphemeralPixels::Owned(pixels))),
            }))
        }
    }

    /// Create a still straight-BGRA image sharing an existing buffer until its
    /// first GPU upload, when this handle drops its reference (on platforms
    /// without D3D11 device-lost atlas invalidation).
    pub fn from_ephemeral_shared_bgra(
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    ) -> Option<Self> {
        let length = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || pixels.len() != length {
            return None;
        }
        #[cfg(target_os = "windows")]
        {
            Some(Self::with_data(RenderImageData::Shared {
                width,
                height,
                pixels,
            }))
        }
        #[cfg(not(target_os = "windows"))]
        {
            Some(Self::with_data(RenderImageData::Ephemeral {
                width,
                height,
                pixels: parking_lot::Mutex::new(Some(EphemeralPixels::Shared(pixels))),
            }))
        }
    }

    fn with_data(data: RenderImageData) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: ImageId(NEXT_ID.fetch_add(1, SeqCst)),
            scale_factor: 1.0,
            data,
        }
    }

    /// Convert this image into a byte slice.
    pub fn as_bytes(&self, frame_index: usize) -> Option<&[u8]> {
        match &self.data {
            RenderImageData::Frames(frames) => frames
                .get(frame_index)
                .map(|frame| frame.buffer().as_raw().as_slice()),
            RenderImageData::Shared { pixels, .. } => (frame_index == 0).then(|| pixels.as_slice()),
            RenderImageData::Ephemeral { .. } => None,
        }
    }

    /// Inspect pixel bytes before GPU upload, including ephemeral images.
    pub fn with_bytes<R>(&self, frame_index: usize, f: impl FnOnce(Option<&[u8]>) -> R) -> R {
        match &self.data {
            RenderImageData::Frames(frames) => f(frames
                .get(frame_index)
                .map(|frame| frame.buffer().as_raw().as_slice())),
            RenderImageData::Shared { pixels, .. } => {
                f((frame_index == 0).then(|| pixels.as_slice()))
            }
            RenderImageData::Ephemeral { pixels, .. } => {
                if frame_index != 0 {
                    return f(None);
                }
                let guard = pixels.lock();
                f(guard.as_ref().map(EphemeralPixels::as_slice))
            }
        }
    }

    /// Clone a shared ephemeral pixel `Arc` before GPU upload so the upload can
    /// borrow the buffer directly without cloning its bytes.
    pub(crate) fn clone_ephemeral_shared(&self, frame_index: usize) -> Option<Arc<Vec<u8>>> {
        if frame_index != 0 {
            return None;
        }
        match &self.data {
            RenderImageData::Ephemeral { pixels, .. } => match pixels.lock().as_ref() {
                Some(EphemeralPixels::Shared(shared)) => Some(shared.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Release ephemeral pixel storage once its bytes have been borrowed for upload.
    pub(crate) fn clear_ephemeral(&self) {
        if let RenderImageData::Ephemeral { pixels, .. } = &self.data {
            *pixels.lock() = None;
        }
    }

    /// Borrow or consume pixel bytes for GPU atlas upload.
    pub(crate) fn take_upload_bytes(&self, frame_index: usize) -> Option<Cow<'_, [u8]>> {
        match &self.data {
            RenderImageData::Frames(frames) => frames
                .get(frame_index)
                .map(|frame| Cow::Borrowed(frame.buffer().as_raw().as_slice())),
            RenderImageData::Shared { pixels, .. } => {
                (frame_index == 0).then(|| Cow::Borrowed(pixels.as_slice()))
            }
            RenderImageData::Ephemeral { pixels, .. } => {
                if frame_index != 0 {
                    return None;
                }
                pixels
                    .lock()
                    .take()
                    .map(|buf| Cow::Owned(buf.into_vec()))
            }
        }
    }

    /// Get the size of this image, in pixels.
    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
        let (width, height) = match &self.data {
            RenderImageData::Frames(frames) => frames[frame_index].buffer().dimensions(),
            RenderImageData::Shared { width, height, .. }
            | RenderImageData::Ephemeral { width, height, .. } => {
                assert_eq!(frame_index, 0);
                (*width, *height)
            }
        };
        size(width.into(), height.into())
    }

    /// Get the size of this image, in pixels for display, adjusted for the scale factor.
    pub(crate) fn render_size(&self, frame_index: usize) -> Size<Pixels> {
        self.size(frame_index)
            .map(|v| (v.0 as f32 / self.scale_factor).into())
    }

    /// Get the delay of this frame from the previous
    pub fn delay(&self, frame_index: usize) -> Delay {
        match &self.data {
            RenderImageData::Frames(frames) => frames[frame_index].delay(),
            RenderImageData::Shared { .. } | RenderImageData::Ephemeral { .. } => {
                assert_eq!(frame_index, 0);
                Delay::from_numer_denom_ms(0, 1)
            }
        }
    }

    /// Get the number of frames for this image.
    pub fn frame_count(&self) -> usize {
        match &self.data {
            RenderImageData::Frames(frames) => frames.len(),
            RenderImageData::Shared { .. } | RenderImageData::Ephemeral { .. } => 1,
        }
    }
}

impl fmt::Debug for RenderImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("id", &self.id)
            .field("size", &self.size(0))
            .finish()
    }
}
