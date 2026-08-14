//! Developer-only measurement for the external portrait corpus.

use super::placeholder::{
    placeholder_distance, thumbnail, PLACEHOLDER_RMSE_MAX, REFERENCE_THUMBNAILS,
};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const CORPUS_ENV: &str = "REPRISE_PORTRAIT_CORPUS_DIR";
const SWEEP_ENV: &str = "REPRISE_PORTRAIT_SWEEP_CSV";
const OUTPUT_ENV: &str = "REPRISE_PORTRAIT_MEASUREMENT_OUTPUT";
const EMPTY_MD5_IDENTIFIER: &str = "d41d8cd98f00b204e9800998ecf8427e";
const OCEANO_IDENTIFIER: &str = "415714b66a5de709809dd3d05f58afe4";
const REQUIRED_PLACEHOLDER_MARGIN: f64 = 10.0;
const REQUIRED_PHOTOGRAPH_MARGIN: f64 = 20.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpectedKind {
    Placeholder,
    Photograph,
}

struct CorpusRow {
    artist: String,
    identifier: String,
    expected: ExpectedKind,
}

#[test]
#[ignore = "reads the external Deezer portrait corpus and writes a requested evidence file"]
fn measure_external_portrait_corpus_and_emit_references() {
    let corpus = required_path(CORPUS_ENV);
    let sweep = required_path(SWEEP_ENV);
    let output = required_path(OUTPUT_ENV);
    let generated_references = [
        read_thumbnail(&corpus, EMPTY_MD5_IDENTIFIER),
        read_thumbnail(&corpus, OCEANO_IDENTIFIER),
    ];
    let rows = read_rows(&sweep);
    let mut report = String::new();

    writeln!(report, "portrait placeholder fingerprint Rust measurement").unwrap();
    writeln!(report, "thumbnail=32x32 grayscale Lanczos3").unwrap();
    writeln!(report, "decode=single shared image decode").unwrap();
    writeln!(report, "distance=normalized RMSE").unwrap();
    writeln!(
        report,
        "required_placeholder_margin={REQUIRED_PLACEHOLDER_MARGIN:.1}x"
    )
    .unwrap();
    writeln!(
        report,
        "required_photograph_margin={REQUIRED_PHOTOGRAPH_MARGIN:.1}x"
    )
    .unwrap();
    writeln!(report, "configured_threshold={PLACEHOLDER_RMSE_MAX:.9}").unwrap();
    for (identifier, reference) in [EMPTY_MD5_IDENTIFIER, OCEANO_IDENTIFIER]
        .into_iter()
        .zip(generated_references)
    {
        writeln!(report, "reference {identifier}").unwrap();
        write_reference(&mut report, &reference);
    }
    writeln!(report, "\nartist\tidentifier\texpected\tdistance").unwrap();

    let mut worst_placeholder = 0.0_f64;
    let mut nearest_photograph = f64::INFINITY;
    let mut placeholders = 0_usize;
    let mut photographs = 0_usize;
    let mut rejected_placeholders = 0_usize;
    let mut rejected_photographs = 0_usize;
    for row in rows {
        let bytes = std::fs::read(corpus.join(format!("{}.jpg", row.identifier)))
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", row.identifier));
        let image = crate::cover_download::decode_image(&bytes)
            .unwrap_or_else(|| panic!("failed to decode {}", row.identifier));
        let distance = placeholder_distance(image.image());
        match row.expected {
            ExpectedKind::Placeholder => {
                placeholders += 1;
                worst_placeholder = worst_placeholder.max(distance);
                rejected_placeholders += usize::from(distance <= PLACEHOLDER_RMSE_MAX);
            }
            ExpectedKind::Photograph => {
                photographs += 1;
                nearest_photograph = nearest_photograph.min(distance);
                rejected_photographs += usize::from(distance <= PLACEHOLDER_RMSE_MAX);
            }
        }
        writeln!(
            report,
            "{}\t{}\t{:?}\t{distance:.9}",
            row.artist, row.identifier, row.expected
        )
        .unwrap();
    }

    let lower_bound = worst_placeholder * REQUIRED_PLACEHOLDER_MARGIN;
    let upper_bound = nearest_photograph / REQUIRED_PHOTOGRAPH_MARGIN;
    writeln!(report, "\nplaceholder_instances={placeholders}").unwrap();
    writeln!(report, "photograph_instances={photographs}").unwrap();
    writeln!(
        report,
        "rejected_placeholder_instances={rejected_placeholders}"
    )
    .unwrap();
    writeln!(
        report,
        "rejected_photograph_instances={rejected_photographs}"
    )
    .unwrap();
    writeln!(report, "worst_placeholder={worst_placeholder:.9}").unwrap();
    writeln!(report, "nearest_photograph={nearest_photograph:.9}").unwrap();
    writeln!(
        report,
        "corpus_separation={:.3}x",
        nearest_photograph / worst_placeholder
    )
    .unwrap();
    writeln!(report, "threshold_lower_bound={lower_bound:.9}").unwrap();
    writeln!(report, "threshold_upper_bound={upper_bound:.9}").unwrap();
    writeln!(
        report,
        "placeholder_margin={:.3}x",
        PLACEHOLDER_RMSE_MAX / worst_placeholder
    )
    .unwrap();
    writeln!(
        report,
        "photograph_margin={:.3}x",
        nearest_photograph / PLACEHOLDER_RMSE_MAX
    )
    .unwrap();
    let margin_window_exists = lower_bound <= upper_bound;
    let configured_threshold_passes =
        margin_window_exists && (lower_bound..=upper_bound).contains(&PLACEHOLDER_RMSE_MAX);
    writeln!(
        report,
        "gate={}",
        if configured_threshold_passes {
            "PASS"
        } else {
            "FAIL"
        }
    )
    .unwrap();
    if !margin_window_exists {
        writeln!(
            report,
            "gate_reason=no threshold can provide the required asymmetric margins"
        )
        .unwrap();
    }

    std::fs::write(&output, report)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));

    assert_eq!(placeholders, 18, "ground-truth placeholder count changed");
    assert_eq!(photographs, 219, "ground-truth photograph count changed");
    assert_eq!(
        rejected_placeholders, placeholders,
        "every ground-truth placeholder must be rejected"
    );
    assert_eq!(
        rejected_photographs, 0,
        "no ground-truth photograph may be rejected"
    );
    assert_eq!(
        REFERENCE_THUMBNAILS, generated_references,
        "embedded reference thumbnails differ; copy them from the evidence output"
    );
    assert!(
        margin_window_exists,
        "required margins do not overlap: threshold must be >= {lower_bound:.9} and <= {upper_bound:.9}"
    );
    assert!(
        (lower_bound..=upper_bound).contains(&PLACEHOLDER_RMSE_MAX),
        "configured threshold {PLACEHOLDER_RMSE_MAX:.9} is outside {lower_bound:.9}..={upper_bound:.9}"
    );
}

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name).map_or_else(
        || panic!("set {name} to run this ignored measurement"),
        PathBuf::from,
    )
}

fn read_thumbnail(corpus: &Path, identifier: &str) -> [u8; 1024] {
    let path = corpus.join(format!("{identifier}.jpg"));
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let image = crate::cover_download::decode_image(&bytes)
        .unwrap_or_else(|| panic!("failed to decode {}", path.display()));
    thumbnail(image.image())
}

fn read_rows(path: &Path) -> Vec<CorpusRow> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .lines()
        .skip(1)
        .filter_map(parse_row)
        .collect()
}

fn parse_row(line: &str) -> Option<CorpusRow> {
    let mut fields = line.rsplitn(6, ',');
    let _rmse64 = fields.next()?;
    let rmse32 = fields.next()?.parse::<f64>().ok()?;
    let _rmse16 = fields.next()?;
    let _fans = fields.next()?;
    let identifier = fields.next()?.to_owned();
    let artist = fields.next()?.trim_matches('"').replace("\"\"", "\"");
    (identifier != "(empty)").then_some(CorpusRow {
        artist,
        identifier,
        expected: if rmse32 <= 0.01 {
            ExpectedKind::Placeholder
        } else {
            ExpectedKind::Photograph
        },
    })
}

fn write_reference(report: &mut String, reference: &[u8; 1024]) {
    writeln!(report, "[").unwrap();
    for row in reference.chunks(32) {
        write!(report, "    ").unwrap();
        for (index, value) in row.iter().enumerate() {
            if index > 0 {
                write!(report, " ").unwrap();
            }
            write!(report, "{value},").unwrap();
        }
        writeln!(report).unwrap();
    }
    writeln!(report, "]").unwrap();
}
