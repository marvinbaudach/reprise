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
// `audioadapter_buffers` is rubato's own re-export, not a second dependency to
// licence-clear: rubato 4 takes its buffers through the `Adapter` traits and
// re-exports the crate that implements them for exactly this reason.
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use reprise_core::stem_separation::StemError;

use crate::chunk::HTDEMUCS_SAMPLE_RATE;
use crate::pcm::{interleave_to_pcm, RENDER_BITS_PER_SAMPLE};

/// Number of channels the model works in.
const STEREO: usize = 2;

/// Input frames the resampler consumes per internal chunk. rubato needs a
/// chunk size up front; 1024 keeps the working set small without making the
/// per-chunk overhead matter on a whole-track resample.
const RESAMPLE_CHUNK_FRAMES: usize = 1024;

/// How far the resample ratio may be adjusted at runtime relative to the one
/// it is built with. Nothing here adjusts it, so this is the smallest value
/// rubato accepts above "no adjustment at all".
const RESAMPLE_MAX_RATIO_RELATIVE: f64 = 1.1;

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
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| StemError::SourceUnreadable(format!("probe: {e}")))?;

    // symphonia 0.6 readers carry video and subtitle tracks too, so "the
    // default track" is only a question you can ask per track type.
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| StemError::SourceUnreadable("no default audio track".to_string()))?;
    let track_id = track.id;
    // Codec parameters are optional in 0.6 — a reader that could not identify
    // the track leaves them `None`, and that track is simply unplayable.
    let params = track
        .codec_params
        .as_ref()
        .and_then(CodecParameters::audio)
        .ok_or_else(|| {
            StemError::SourceUnreadable("audio track has no codec parameters".to_string())
        })?;
    let mut rate = params.sample_rate.unwrap_or(HTDEMUCS_SAMPLE_RATE);
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| StemError::SourceUnreadable(format!("no decoder: {e}")))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut planes: Vec<Vec<f32>> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            // 0.6 reports the end of the media as `Ok(None)`. An I/O error is
            // now always a real read failure — 0.5 delivered EOF as one, which
            // is why this arm used to swallow it.
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(StemError::SourceUnreadable(format!("read packet: {e}"))),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A single corrupt packet is skipped, not fatal.
            Err(SymphoniaError::DecodeError(_)) => continue,
            // A truncated final packet still leaves everything decoded so far
            // usable; failing the whole job over the tail would be worse.
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(StemError::SourceUnreadable(format!("decode: {e}"))),
        };
        rate = decoded.spec().rate();
        decoded.copy_to_vecs_planar(&mut planes);
        append_planes(&mut channels, &planes)?;
    }

    if channels.is_empty() || channels[0].is_empty() {
        return Err(StemError::SourceUnreadable(
            "decoded no audio samples".to_string(),
        ));
    }
    Ok((channels, rate))
}

/// Appends one decoded packet's planes to the per-channel lanes, allocating the
/// lanes on the first packet.
///
/// symphonia 0.6 hands the packet over already planar, so the interleaved split
/// this used to do is gone — and with it the `i % 0` division a zero-channel
/// packet once threatened. The zero-channel rejection stays anyway: a crafted or
/// corrupt file can still present a packet with no planes at all, and accepting
/// it silently would surface as an empty decode much further downstream.
///
/// A packet whose plane count differs from the first is left to the caller's
/// backend-panic guard; `zip` simply ignores the surplus. Real files never vary
/// it.
fn append_planes(channels: &mut Vec<Vec<f32>>, planes: &[Vec<f32>]) -> Result<(), StemError> {
    if planes.is_empty() {
        return Err(StemError::SourceUnreadable(
            "decoded audio reports zero channels".to_string(),
        ));
    }
    if channels.is_empty() {
        channels.resize(planes.len(), Vec::new());
    }
    for (lane, plane) in channels.iter_mut().zip(planes) {
        lane.extend_from_slice(plane);
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
/// polynomial resampler (cubic).
///
/// rubato 4 owns the chunk loop this function used to write by hand:
/// `process_all_into_buffer` sizes and feeds the chunks, and trims the
/// resampler's startup delay off the front. That delay is one frame for this
/// configuration, so nothing audible changes — the point is that the trim is
/// now the library's job and cannot silently drift out of sync with the
/// truncation that follows it.
fn resample_stereo(
    stereo: &[Vec<f32>],
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<Vec<f32>>, StemError> {
    let ratio = f64::from(to_rate) / f64::from(from_rate);
    let mut resampler = Async::<f32>::new_poly(
        ratio,
        RESAMPLE_MAX_RATIO_RELATIVE,
        PolynomialDegree::Cubic,
        RESAMPLE_CHUNK_FRAMES,
        STEREO,
        FixedAsync::Input,
    )
    .map_err(|e| StemError::Backend(format!("resampler init: {e}")))?;

    let total = stereo[0].len();
    // Rejects a ragged pair rather than reading past the shorter lane.
    let input = SequentialSliceOfVecs::new(stereo, STEREO, total)
        .map_err(|e| StemError::Backend(format!("resample input: {e}")))?;

    let capacity = resampler.process_all_needed_output_len(total);
    let mut planes = vec![vec![0.0f32; capacity]; STEREO];
    let mut output = SequentialSliceOfVecs::new_mut(&mut planes, STEREO, capacity)
        .map_err(|e| StemError::Backend(format!("resample output: {e}")))?;

    let (_consumed, produced) = resampler
        .process_all_into_buffer(&input, &mut output, total, None)
        .map_err(|e| StemError::Backend(format!("resample: {e}")))?;

    for plane in &mut planes {
        plane.truncate(produced);
    }
    Ok(planes)
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
        // The regression: a crafted/corrupt file whose decoded packet carries
        // no planes at all must yield a clean SourceUnreadable, never take the
        // worker process down.
        let mut channels: Vec<Vec<f32>> = Vec::new();
        let result = append_planes(&mut channels, &[]);
        assert!(matches!(result, Err(StemError::SourceUnreadable(_))));
        assert!(channels.is_empty(), "a rejected packet allocates no lanes");
    }

    #[test]
    fn planes_are_appended_to_their_own_lanes() {
        let mut channels: Vec<Vec<f32>> = Vec::new();
        append_planes(&mut channels, &[vec![1.0], vec![-1.0]]).unwrap();
        // A second packet extends the same lanes rather than starting new ones.
        append_planes(&mut channels, &[vec![2.0], vec![-2.0]]).unwrap();
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
