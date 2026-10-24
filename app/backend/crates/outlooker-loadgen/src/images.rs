//! Generated inline images (spec Part 04 §12.3): indexes `0..logos` are small shared logos, the rest a pool of
//! gradient + light-noise PNG / JPEG images. Each image is a pure function of (seed, index).
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Mutex;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use outlooker_core::importer::types::InlineImageOut;
use outlooker_core::importer::webp::{convert, ImageIn, ImageOut, KnownHashes};

use crate::rng::Rng;

pub const WEBP_QUALITY: u8 = 80;

pub struct ImagePool {
    seed: u64,
    logos: u32,
    max_side: u32,
    /// index → source hash of images generated before; lets a known image skip generation entirely.
    hashes: Mutex<HashMap<u32, [u8; 32]>>,
}

impl ImagePool {
    pub fn new(seed: u64, logos: u32, max_side: u32) -> ImagePool {
        ImagePool { seed, logos, max_side, hashes: Mutex::new(HashMap::new()) }
    }

    /// Source bytes and MIME type of image `index`.
    pub fn source(&self, index: u32) -> (Vec<u8>, &'static str) {
        let mut rng = Rng::new(self.seed ^ 0x1A6E ^ ((index as u64) << 20));
        let (lo, hi) = if index < self.logos { (64, 200) } else { (50, self.max_side as u64) };
        let (w, h) = (rng.range(lo, hi) as u32, rng.range(lo, hi) as u32);
        let base = [rng.below(256) as u8, rng.below(256) as u8, rng.below(256) as u8];
        let noise_seed = rng.next_u64();
        let img = RgbImage::from_fn(w, h, |x, y| {
            let n = (((x as u64).wrapping_mul(73_856_093) ^ (y as u64).wrapping_mul(19_349_663) ^ noise_seed) % 17) as u8;
            Rgb([
                base[0].wrapping_add((x * 255 / w) as u8 / 2).wrapping_add(n),
                base[1].wrapping_add((y * 255 / h) as u8 / 2).wrapping_add(n),
                base[2].wrapping_add(((x + y) * 255 / (w + h)) as u8 / 2),
            ])
        });
        let (format, mime) = if index.is_multiple_of(2) { (ImageFormat::Png, "image/png") } else { (ImageFormat::Jpeg, "image/jpeg") };
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut out), format).expect("encode generated image");
        (out, mime)
    }

    /// The inline-image row for image `index` at `ordinal`; `None` when `webp::convert` drops it (Tiny / Rejected).
    pub fn inline(&self, index: u32, ordinal: i32, known: &KnownHashes) -> Option<InlineImageOut> {
        let content_id = Some(format!("img{index}@loadgen.example"));
        let cached = self.hashes.lock().unwrap_or_else(|p| p.into_inner()).get(&index).copied();
        if let Some(h) = cached.filter(|h| known.contains(h)) {
            return Some(InlineImageOut { ordinal, content_id, source_hash: h, blob: None });
        }
        let (bytes, _) = self.source(index);
        match convert(&ImageIn { bytes }, WEBP_QUALITY, known) {
            ImageOut::Encoded { source_hash, image } => {
                self.hashes.lock().unwrap_or_else(|p| p.into_inner()).insert(index, source_hash);
                Some(InlineImageOut { ordinal, content_id, source_hash, blob: Some(image) })
            }
            ImageOut::Known { source_hash } => {
                self.hashes.lock().unwrap_or_else(|p| p.into_inner()).insert(index, source_hash);
                Some(InlineImageOut { ordinal, content_id, source_hash, blob: None })
            }
            ImageOut::Tiny | ImageOut::Rejected { .. } => None,
        }
    }
}
