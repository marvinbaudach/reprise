//! Audio decode / resample / encode for the real backend (feature `ort`).
//!
//! * **Decode** (symphonia, MPL-2.0): any library format → planar f32 at its
//!   native rate, downmixed/upmixed to stereo.
//! * **Resample** (rubato, MIT): to htdemucs' native 44.1 kHz when the source
//!   differs — the reference `infer.py` requires the caller to do this.
//! * **Encode** (flacenc, Apache-2.0): the instrumental → 24-bit FLAC.
//!
//! These are the only crates that touch native audio formats, and all are
//! pure-Rust, so the whole path stays Flatpak-offline buildable.

use std::path::Path;

use flacenc::component::BitRepr;
use flacenc::error::Verify;
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use reprise_core::stem_separation::StemError;

use crate::chunk::HTDEMUCS_SAMPLE_RATE;
use crate::pcm::{interleave_to_pcm, RENDER_BITS_PER_SAMPLE};

/// Number of channels the model works in.
const STEREO: usize = 2;

/// Decodes `path` to stereo planar f32 at [`HTDEMUCS_SAMPLE_RATE`]. Mono is
/// duplicated to stereo (as the reference does); more than two channels keeps
/// the front L/R pair.
pub fn decode_to_stereo_44100(path: &Path) -> Result<Vec<Vec<f32>>, StemError> {
    let (planar, source_rate) = decode_native(path)?;
    let stereo = to_stereo(planar);
    if source_rate == HTDEMUCS_SAMPLE_RATE {
        return Ok(stereo);
    }
    resample_stereo(&stereo, source_rate, HTDEMUCS_SAMPLE_RATE)
}

/// Decodes every packet to planar f32 at the file's native rate, returning the
/// channels and that rate.
fn decode_native(path: &Path) -> Result<(Vec<Vec<f32>>, u32), StemError> {
    let file = std::fs::File::open(path)
        .map_err(|e| StemError::SourceUnreadable(format!("open {}: {e}", path.display())))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| StemError::SourceUnreadable(format!("probe: {e}")))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| StemError::SourceUnreadable("no default audio track".to_string()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| StemError::SourceUnreadable(format!("no decoder: {e}")))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut rate = track
        .codec_params
        .sample_rate
        .unwrap_or(HTDEMUCS_SAMPLE_RATE);

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // Any read error ends the stream (EOF surfaces as an IoError here).
            Err(SymphoniaError::IoError(_) | SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(StemError::SourceUnreadable(format!("read packet: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A single corrupt packet is skipped, not fatal.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(StemError::SourceUnreadable(format!("decode: {e}"))),
        };
        let spec = *decoded.spec();
        let channel_count = spec.channels.count();
        rate = spec.rate;
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        push_interleaved(&mut channels, buffer.samples(), channel_count)?;
    }

    if channels.is_empty() || channels[0].is_empty() {
        return Err(StemError::SourceUnreadable(
            "decoded no audio samples".to_string(),
        ));
    }
    Ok((channels, rate))
}

/// Splits one decoded packet's interleaved samples into per-channel lanes,
/// allocating the lanes on the first packet. A crafted or corrupt file can
/// present a packet claiming **zero** channels; reject it as unreadable rather
/// than dividing by zero (the old `i % 0`). Deriving the divisor `lanes` from
/// the same `channel_count.max(1)` expression that sizes the lanes keeps the two
/// provably identical, so the modulo can never divide by zero even if the guard
/// above were relaxed. (A packet whose channel count differs from the first is
/// left to the caller's backend-panic guard; real files never vary it.)
fn push_interleaved(
    channels: &mut Vec<Vec<f32>>,
    samples: &[f32],
    channel_count: usize,
) -> Result<(), StemError> {
    if channel_count == 0 {
        return Err(StemError::SourceUnreadable(
            "decoded audio reports zero channels".to_string(),
        ));
    }
    let lanes = channel_count.max(1);
    if channels.is_empty() {
        *channels = vec![Vec::new(); lanes];
    }
    for (i, &sample) in samples.iter().enumerate() {
        channels[i % lanes].push(sample);
    }
    Ok(())
}

/// Collapses arbitrary channel counts to stereo: mono is duplicated, stereo is
/// kept, and more than two channels keeps the front L/R pair.
fn to_stereo(mut planar: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
    match planar.len() {
        0 => vec![Vec::new(), Vec::new()],
        1 => {
            let mono = planar.pop().unwrap();
            vec![mono.clone(), mono]
        }
        _ => {
            planar.truncate(STEREO);
            planar
        }
    }
}

/// Resamples stereo planar audio from `from_rate` to `to_rate` with rubato's
/// polynomial resampler (cubic). Output length is the exact ratio-scaled count.
fn resample_stereo(
    stereo: &[Vec<f32>],
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<Vec<f32>>, StemError> {
    let ratio = f64::from(to_rate) / f64::from(from_rate);
    let chunk = 1024usize;
    let mut resampler = FastFixedIn::<f32>::new(ratio, 1.1, PolynomialDegree::Cubic, chunk, STEREO)
        .map_err(|e| StemError::Backend(format!("resampler init: {e}")))?;

    let total = stereo[0].len();
    let mut out_left = Vec::with_capacity((total as f64 * ratio) as usize + chunk);
    let mut out_right = Vec::with_capacity((total as f64 * ratio) as usize + chunk);

    let mut position = 0usize;
    while position < total {
        let need = resampler.input_frames_next();
        let end = (position + need).min(total);
        let mut left = stereo[0][position..end].to_vec();
        let mut right = stereo[1][position..end].to_vec();
        left.resize(need, 0.0);
        right.resize(need, 0.0);
        let out = resampler
            .process(&[left, right], None)
            .map_err(|e| StemError::Backend(format!("resample: {e}")))?;
        out_left.extend_from_slice(&out[0]);
        out_right.extend_from_slice(&out[1]);
        position += need;
    }

    let expected = (total as f64 * ratio).round() as usize;
    out_left.truncate(expected);
    out_right.truncate(expected);
    Ok(vec![out_left, out_right])
}

/// Encodes stereo planar f32 to a 24-bit FLAC file at `HTDEMUCS_SAMPLE_RATE`.
pub fn encode_flac(path: &Path, stereo: &[Vec<f32>]) -> Result<(), StemError> {
    let channels = stereo.len();
    let samples = interleave_to_pcm(stereo, RENDER_BITS_PER_SAMPLE);

    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|_| StemError::Backend("invalid FLAC encoder config".to_string()))?;
    let source = flacenc::source::MemSource::from_samples(
        &samples,
        channels,
        RENDER_BITS_PER_SAMPLE as usize,
        HTDEMUCS_SAMPLE_RATE as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| StemError::Backend(format!("FLAC encode: {e}")))?;

    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| StemError::Backend(format!("FLAC serialise: {e}")))?;

    write_atomic(path, sink.as_slice())
}

/// Publishes the render atomically (temp file + rename), so a crash mid-write
/// never leaves a half-written FLAC at the output path.
fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), StemError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    let tmp = parent.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| StemError::Io(e.to_string()))?;
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        StemError::Io(e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_channel_packet_is_rejected_not_a_panic() {
        // The regression: a crafted/corrupt file whose decoded packet claims
        // zero channels must yield a clean SourceUnreadable, never an `i % 0`
        // panic that would take the worker process down.
        let mut channels: Vec<Vec<f32>> = Vec::new();
        let result = push_interleaved(&mut channels, &[0.1, 0.2, 0.3], 0);
        assert!(matches!(result, Err(StemError::SourceUnreadable(_))));
        assert!(channels.is_empty(), "a rejected packet allocates no lanes");
    }

    #[test]
    fn interleaved_samples_are_split_into_channel_lanes() {
        let mut channels: Vec<Vec<f32>> = Vec::new();
        // Two channels: [L0, R0, L1, R1] -> L = [L0, L1], R = [R0, R1].
        push_interleaved(&mut channels, &[1.0, -1.0, 2.0, -2.0], 2).unwrap();
        assert_eq!(channels, vec![vec![1.0, 2.0], vec![-1.0, -2.0]]);
    }

    #[test]
    fn mono_is_duplicated_to_stereo() {
        let stereo = to_stereo(vec![vec![0.1, 0.2, 0.3]]);
        assert_eq!(stereo.len(), 2);
        assert_eq!(stereo[0], stereo[1]);
        assert_eq!(stereo[0], vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn more_than_two_channels_keeps_the_front_pair() {
        let stereo = to_stereo(vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0]]);
        assert_eq!(stereo, vec![vec![1.0], vec![2.0]]);
    }

    #[test]
    fn resampling_scales_the_length_by_the_ratio() {
        // 48 kHz -> 44.1 kHz: 4800 frames -> ~4410.
        let stereo = vec![vec![0.0f32; 4800], vec![0.0f32; 4800]];
        let out = resample_stereo(&stereo, 48_000, 44_100).unwrap();
        let expected = (4800.0_f64 * 44_100.0 / 48_000.0).round() as usize;
        assert_eq!(out[0].len(), expected);
        assert_eq!(out[1].len(), expected);
    }

    #[test]
    fn encode_flac_writes_valid_streaminfo() {
        // Encode and verify the FLAC's STREAMINFO directly (independent of any
        // decoder): the render must be a real fLaC stream at 44.1 kHz / stereo /
        // 24-bit with the right sample count. flacenc's output is
        // reference-valid (ffmpeg, `flac -t` and lofty — the promotion tag path
        // — all read it); its variable block size only trips symphonia's
        // probe, which never reads our output in production (GStreamer does).
        let n = HTDEMUCS_SAMPLE_RATE as usize / 10; // 0.1 s
        let tone: Vec<f32> = (0..n).map(|i| 0.25 * (i as f32 * 0.05).sin()).collect();
        let stereo = vec![tone.clone(), tone];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.flac");
        encode_flac(&path, &stereo).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"fLaC", "must start with the FLAC marker");
        let info = parse_streaminfo(&bytes);
        assert_eq!(info.sample_rate, HTDEMUCS_SAMPLE_RATE);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, RENDER_BITS_PER_SAMPLE);
        assert_eq!(info.total_samples, n as u64);
    }

    #[test]
    fn decode_reads_a_wav_to_stereo_at_the_model_rate() {
        // A real 44.1 kHz stereo WAV decoded end-to-end (symphonia reads normal
        // files fine — the decode path). No resample happens at the model rate.
        let n = 2000usize;
        let left: Vec<i16> = (0..n)
            .map(|i| ((i as f32 * 0.1).sin() * 10_000.0) as i16)
            .collect();
        let right: Vec<i16> = left.iter().map(|s| -s).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("in.wav");
        std::fs::write(&path, wav_stereo_44100(&left, &right)).unwrap();

        let stereo = decode_to_stereo_44100(&path).unwrap();
        assert_eq!(stereo.len(), 2);
        assert_eq!(stereo[0].len(), n);
        // First sample is 0 in both channels; a mid sample keeps L = -R.
        assert!((stereo[0][500] + stereo[1][500]).abs() < 1e-3);
    }

    #[test]
    fn decoding_a_non_audio_file_is_a_clean_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bogus.flac");
        std::fs::write(&path, b"not audio at all").unwrap();
        assert!(matches!(
            decode_to_stereo_44100(&path),
            Err(StemError::SourceUnreadable(_))
        ));
    }

    struct StreamInfo {
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
        total_samples: u64,
    }

    /// Parses the FLAC STREAMINFO fields that follow the 4-byte `fLaC` marker
    /// and the 4-byte metadata-block header.
    fn parse_streaminfo(bytes: &[u8]) -> StreamInfo {
        let b = &bytes[8..]; // STREAMINFO body
        let sample_rate =
            (u32::from(b[10]) << 12) | (u32::from(b[11]) << 4) | (u32::from(b[12]) >> 4);
        let channels = ((u32::from(b[12]) >> 1) & 0x7) + 1;
        let bits_per_sample = (((u32::from(b[12]) & 1) << 4) | (u32::from(b[13]) >> 4)) + 1;
        let total_samples = (u64::from(b[13] & 0xF) << 32)
            | (u64::from(b[14]) << 24)
            | (u64::from(b[15]) << 16)
            | (u64::from(b[16]) << 8)
            | u64::from(b[17]);
        StreamInfo {
            sample_rate,
            channels,
            bits_per_sample,
            total_samples,
        }
    }

    /// Builds a minimal PCM16 stereo 44.1 kHz WAV.
    fn wav_stereo_44100(left: &[i16], right: &[i16]) -> Vec<u8> {
        let frames = left.len();
        let data_len = (frames * 2 * 2) as u32; // 2 channels * 2 bytes
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&2u16.to_le_bytes()); // channels
        out.extend_from_slice(&44_100u32.to_le_bytes());
        out.extend_from_slice(&(44_100u32 * 4).to_le_bytes()); // byte rate
        out.extend_from_slice(&4u16.to_le_bytes()); // block align
        out.extend_from_slice(&16u16.to_le_bytes()); // bits
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for (l, r) in left.iter().zip(right) {
            out.extend_from_slice(&l.to_le_bytes());
            out.extend_from_slice(&r.to_le_bytes());
        }
        out
    }
}
