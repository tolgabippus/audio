//! Decodes a file's audio (or a video's sound track) to 16 kHz mono, the
//! format every recording is transcribed and kept in.

use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::resample::MonoResampler;

/// The whole of `path`'s first audio track, downmixed to mono and resampled
/// to 16 kHz.
pub fn decode_audio_file(path: &Path) -> Result<Vec<f32>> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("Not a supported audio or video file: {}", e))?
        .format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("The file has no audio track"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("The file's audio format is not supported: {}", e))?;

    // Built on the first packet, which gives the sample rate.
    let mut resampled: Option<MonoResampler> = None;
    let mut buffer: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // The end of the stream.
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(e) => return Err(anyhow!("Failed to read the file: {}", e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged packet loses only its own audio.
            Err(SymphoniaError::DecodeError(e)) => {
                warn!("Skipping an undecodable packet: {}", e);
                continue;
            }
            Err(e) => return Err(anyhow!("Failed to decode the file: {}", e)),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let resampled = match &mut resampled {
            None => resampled.insert(MonoResampler::new(spec.rate)?),
            Some(r) if r.rate() != spec.rate => {
                return Err(anyhow!("The file's sample rate changes partway through"));
            }
            Some(r) => r,
        };
        let packet_frames = decoded.capacity() as u64;
        if buffer
            .as_ref()
            .is_none_or(|b| b.capacity() < decoded.capacity() * channels)
        {
            buffer = Some(SampleBuffer::new(packet_frames, spec));
        }
        let buffer = buffer.as_mut().expect("buffer was just sized");
        buffer.copy_interleaved_ref(decoded);
        resampled.push(
            buffer
                .samples()
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        )?;
    }

    let resampled = resampled.ok_or_else(|| anyhow!("The file has no audio"))?;
    debug!(
        "Decoded {:.1}s of audio at {} Hz from {}",
        resampled.input_len() as f64 / resampled.rate() as f64,
        resampled.rate(),
        path.display()
    );
    resampled.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};

    #[test]
    fn stereo_44k_file_becomes_16k_mono() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        for sample in tone(44_100, 88_200) {
            let value = (sample * i16::MAX as f32) as i16;
            writer.write_sample(value).unwrap();
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();

        let samples = decode_audio_file(&path).unwrap();

        assert_eq!(samples.len(), 32_000);
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((0.4..0.6).contains(&peak), "peak {}", peak);
    }

    fn tone(rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| (i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect()
    }

    #[test]
    fn a_file_that_is_not_audio_fails_to_decode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "not audio at all").unwrap();
        assert!(decode_audio_file(&path).is_err());
    }
}
