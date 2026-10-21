//! Part 04 §5.7 inline image → WebP.
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::RwLock;

use image::{imageops::FilterType, ImageFormat, ImageReader, Limits};

use crate::importer::types::EncodedImage;

pub struct ImageIn { pub bytes: Vec<u8> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageOut {
    Encoded { source_hash: [u8; 32], image: EncodedImage },
    Known { source_hash: [u8; 32] },
    Tiny,
    Rejected { message: String },
}

#[derive(Default)]
pub struct KnownHashes { inner: RwLock<HashSet<[u8; 32]>> }

impl KnownHashes {
    pub fn new() -> KnownHashes { KnownHashes::default() }
    pub fn contains(&self, h: &[u8; 32]) -> bool { self.inner.read().unwrap().contains(h) }
    pub fn insert_all<I: IntoIterator<Item = [u8; 32]>>(&self, hashes: I) { self.inner.write().unwrap().extend(hashes) }
    pub fn len(&self) -> usize { self.inner.read().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

pub const MAX_SOURCE_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 40_000_000;
pub const MAX_SIDE: u32 = 2048;

pub fn source_hash(bytes: &[u8]) -> [u8; 32] { *blake3::hash(bytes).as_bytes() }

pub fn convert(input: &ImageIn, quality: u8, known: &KnownHashes) -> ImageOut {
    let source_hash = source_hash(&input.bytes);
    if known.contains(&source_hash) {
        return ImageOut::Known { source_hash };
    }
    if input.bytes.len() > MAX_SOURCE_BYTES {
        return ImageOut::Rejected { message: format!("image source larger than 25 MB ({} bytes)", input.bytes.len()) };
    }
    let format = match image::guess_format(&input.bytes) {
        Ok(f) => f,
        Err(e) => return ImageOut::Rejected { message: format!("unknown image format: {e}") },
    };
    let source_mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Bmp => "image/bmp",
        other => return ImageOut::Rejected { message: format!("unsupported image format {other:?}") },
    };
    let reader = || {
        let mut r = ImageReader::new(Cursor::new(&input.bytes[..]));
        r.set_format(format);
        let mut limits = Limits::default();
        limits.max_alloc = Some(MAX_PIXELS * 4 + 1024 * 1024);
        r.limits(limits);
        r
    };
    let (w, h) = match reader().into_dimensions() {
        Ok(d) => d,
        Err(e) => return ImageOut::Rejected { message: format!("image header unreadable: {e}") },
    };
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return ImageOut::Rejected { message: format!("image larger than 40 megapixels ({w}x{h})") };
    }
    if w <= 2 || h <= 2 {
        return ImageOut::Tiny;
    }
    let mut img = match reader().decode() {
        Ok(i) => i,
        Err(e) => return ImageOut::Rejected { message: format!("image decode failed: {e}") },
    };
    if w.max(h) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let q = f32::from(quality.clamp(1, 100));
    let webp = webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(q).to_vec();
    ImageOut::Encoded {
        source_hash,
        image: EncodedImage { webp, width: w as i32, height: h as i32, source_mime: source_mime.to_string() },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    pub(crate) fn png(w: u32, h: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// 54-byte BMP header (24-bit, uncompressed) with no pixel data.
    fn bmp_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = Vec::with_capacity(54);
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(54 + w * h * 3).to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 24]);
        b
    }

    #[test]
    fn known_hash_skips_encode() {
        let bytes = png(16, 16);
        let known = KnownHashes::new();
        let first = convert(&ImageIn { bytes: bytes.clone() }, 80, &known);
        let ImageOut::Encoded { source_hash, image } = first else { panic!("expected Encoded") };
        assert_eq!(source_hash, super::source_hash(&bytes));
        assert_eq!((image.width, image.height), (16, 16));
        assert_eq!(&image.webp[0..4], b"RIFF");
        known.insert_all([source_hash]);
        assert_eq!(convert(&ImageIn { bytes }, 80, &known), ImageOut::Known { source_hash });
    }

    #[test]
    fn tiny_image() {
        assert_eq!(convert(&ImageIn { bytes: png(1, 1) }, 80, &KnownHashes::new()), ImageOut::Tiny);
    }

    #[test]
    fn resize_to_2048() {
        let out = convert(&ImageIn { bytes: png(3000, 1000) }, 80, &KnownHashes::new());
        let ImageOut::Encoded { image, .. } = out else { panic!("expected Encoded") };
        assert_eq!((image.width, image.height), (2048, 683));
    }

    #[test]
    fn limits_rejected() {
        // 7000x6000 = 42 MP: the check runs on the header only, so a BMP header (no checksum) is enough.
        let out = convert(&ImageIn { bytes: bmp_header(7000, 6000) }, 80, &KnownHashes::new());
        let ImageOut::Rejected { message } = out else { panic!("expected Rejected") };
        assert!(message.contains("40 megapixels"), "{message}");
        let mut big = png(4, 4);
        big.resize(26 * 1024 * 1024, 0);
        assert!(matches!(convert(&ImageIn { bytes: big }, 80, &KnownHashes::new()), ImageOut::Rejected { .. }));
        let tiff = b"II*\0garbage".to_vec();
        assert!(matches!(convert(&ImageIn { bytes: tiff }, 80, &KnownHashes::new()), ImageOut::Rejected { .. }));
    }
}
