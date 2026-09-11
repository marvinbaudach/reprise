use super::*;

/// Renders the head of the panel to a PPM so the light can be looked at.
#[test]
#[ignore = "measurement: render manually via xvfb-run"]
fn render_cover_cloud_gallery_ppm() {
    gtk4::init().expect("gtk");
    let width = 300i32;
    let band = tokens::NOW_PLAYING_ARTWORK_BAND;
    let moments = [0.0f64, 40.0, 80.0];
    let covers = [swatch_cover(false), swatch_cover(true)];
    let sheet = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        width * moments.len() as i32,
        band * covers.len() as i32,
    )
    .expect("sheet");
    let sheet_cr = cairo::Context::new(&sheet).expect("sheet cr");

    for (row, texture) in covers.iter().enumerate() {
        let back = build_blob_rasters(texture, BACK_BLUR_EDGE, &BACK_BLOBS).expect("back drops");
        let front =
            build_blob_rasters(texture, FRONT_BLUR_EDGE, &FRONT_BLOBS).expect("front drops");
        for (col, seconds) in moments.iter().enumerate() {
            let tile =
                cairo::ImageSurface::create(cairo::Format::ARgb32, width, band).expect("tile");
            let cr = cairo::Context::new(&tile).expect("tile cr");
            let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
            cr.set_source_rgb(
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            );
            cr.paint().expect("ground");
            let bounds = field(f64::from(width), f64::from(tokens::NOW_PLAYING_COVER_SIZE));
            let operator = blend_operator(crate::ui::style::accent::is_dark());
            for (surface, blob) in back.iter().zip(BACK_BLOBS) {
                paint_layer(
                    &cr,
                    surface,
                    (blob.x, blob.y),
                    drift_at(*seconds, blob.drift),
                    bounds,
                    1.0,
                    operator,
                );
            }
            for (surface, blob) in front.iter().zip(FRONT_BLOBS) {
                paint_layer(
                    &cr,
                    surface,
                    (blob.x, blob.y),
                    drift_at(*seconds, blob.drift),
                    bounds,
                    1.0,
                    operator,
                );
            }
            let scrim = build_scrim(f64::from(band));
            paint_scrim(&cr, f64::from(width), f64::from(band), &scrim);
            drop(cr);
            sheet_cr
                .set_source_surface(
                    &tile,
                    f64::from(width) * col as f64,
                    f64::from(band) * row as f64,
                )
                .expect("place tile");
            sheet_cr.paint().expect("paint tile");
        }
    }
    drop(sheet_cr);
    let path = std::env::var("COVER_CLOUD_PPM")
        .unwrap_or_else(|_| "/tmp/cover-cloud-gallery.ppm".to_string());
    write_ppm(sheet, &path);
    println!("wrote {path}");
}

#[test]
#[ignore = "measurement: run manually via xvfb-run"]
fn measure_cover_cloud_raster_build_cost() {
    gtk4::init().expect("gtk");
    let texture = swatch_cover(false);
    let samples = 40;
    let started = std::time::Instant::now();
    for _ in 0..samples {
        std::hint::black_box(build_blob_rasters(&texture, BACK_BLUR_EDGE, &BACK_BLOBS));
        std::hint::black_box(build_blob_rasters(&texture, FRONT_BLUR_EDGE, &FRONT_BLOBS));
    }
    let elapsed = started.elapsed();
    println!(
        "cover-change raster build: {:.3} ms ({samples} samples)",
        elapsed.as_secs_f64() * 1_000.0 / f64::from(samples)
    );
    for (name, blob) in ["back 1", "back 2", "back 3"]
        .into_iter()
        .zip(BACK_BLOBS)
        .chain(
            ["front 1", "front 2", "front 3"]
                .into_iter()
                .zip(FRONT_BLOBS),
        )
    {
        let (peak_x, peak_y) =
            super::super::drift_tests::peak_translation_speed(blob.drift, 0.01, 600.0);
        println!("{name}: peak x={peak_x:.9}, y={peak_y:.9}");
    }
}
