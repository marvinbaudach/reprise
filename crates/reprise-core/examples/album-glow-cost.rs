use std::io::Cursor;
use std::time::Instant;

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use reprise_core::cover::{blur_reduced_thumbnail, thumbnail, CoverSource, ThumbnailSize};
use serde::Serialize;

const CACHE_SAMPLES: usize = 1_000;

#[derive(Serialize)]
struct AlbumGlowCost {
    schema_version: u32,
    source_pixels: u64,
    texture_pixels: u64,
    decoded_texture_bytes: u64,
    cold_downscale_us: u64,
    cold_blur_us: u64,
    one_time_total_us: u64,
    cache_hit_p95_ns: u64,
    cache_samples: usize,
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn p95(mut samples: Vec<u64>) -> u64 {
    samples.sort_unstable();
    let rank = (samples.len() * 95).div_ceil(100);
    samples[rank.saturating_sub(1)]
}

fn fixture() -> Vec<u8> {
    let image = ImageBuffer::from_fn(1_200, 1_200, |x, y| {
        let red = u8::try_from((x * 255) / 1_199).unwrap_or(255);
        let blue = u8::try_from((y * 255) / 1_199).unwrap_or(255);
        Rgba([red, 64, blue, 255])
    });
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode generated benchmark cover");
    bytes.into_inner()
}

fn main() {
    let source = CoverSource::Embedded(fixture());
    let downscale_started = Instant::now();
    let reduced = thumbnail(&source, ThumbnailSize::Glow).expect("downscale benchmark cover");
    let cold_downscale_us = elapsed_us(downscale_started);

    let blur_started = Instant::now();
    blur_reduced_thumbnail(&reduced, 6.0).expect("preblur benchmark cover");
    let cold_blur_us = elapsed_us(blur_started);

    let cache_samples = (0..CACHE_SAMPLES)
        .map(|_| {
            let started = Instant::now();
            blur_reduced_thumbnail(&reduced, 6.0).expect("reuse cached preblur");
            elapsed_ns(started)
        })
        .collect();
    let report = AlbumGlowCost {
        schema_version: 1,
        source_pixels: 1_200 * 1_200,
        texture_pixels: 32 * 32,
        decoded_texture_bytes: 32 * 32 * 4,
        cold_downscale_us,
        cold_blur_us,
        one_time_total_us: cold_downscale_us.saturating_add(cold_blur_us),
        cache_hit_p95_ns: p95(cache_samples),
        cache_samples: CACHE_SAMPLES,
    };
    serde_json::to_writer_pretty(std::io::stdout(), &report).expect("write benchmark report");
    println!();
}
