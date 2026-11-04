//! One meeting track (microphone or system audio) written to disk as it
//! arrives: 16 kHz mono WAV, its header kept current so a crash loses only
//! the last few seconds, and gaps filled with silence so both tracks stay
//! aligned to the meeting's start.

use anyhow::Result;
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;

/// How often the WAV header is rewritten and the data flushed to disk.
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
/// A track this far behind the clock gets silence, so a stream that stopped
/// or started late doesn't shift the rest of the track.
const GAP_TOLERANCE_SAMPLES: u64 = WHISPER_SAMPLE_RATE as u64 / 4;

pub struct TrackWriter {
    writer: WavWriter<BufWriter<File>>,
    /// Samples written, silence included.
    written: u64,
    last_flush: Instant,
}

impl TrackWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: WHISPER_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(path, spec)?;
        // A valid, empty file from the start.
        writer.flush()?;
        Ok(Self {
            writer,
            written: 0,
            last_flush: Instant::now(),
        })
    }

    /// Appends `samples`, captured `offset` after the meeting started. If the
    /// track is behind that point, the gap is filled with silence first.
    pub fn write(&mut self, samples: &[f32], offset: Duration) -> Result<()> {
        let due = (offset.as_secs_f64() * WHISPER_SAMPLE_RATE as f64) as u64;
        if due > self.written + GAP_TOLERANCE_SAMPLES {
            for _ in self.written..due {
                self.writer.write_sample(0i16)?;
            }
            self.written = due;
        }
        for &sample in samples {
            self.writer
                .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        self.written += samples.len() as u64;
        if self.last_flush.elapsed() >= FLUSH_INTERVAL {
            self.writer.flush()?;
            self.last_flush = Instant::now();
        }
        Ok(())
    }

    /// Samples written so far, silence included.
    #[cfg(test)]
    pub fn written(&self) -> u64 {
        self.written
    }

    pub fn finalize(self) -> Result<()> {
        self.writer.finalize()?;
        Ok(())
    }
}

/// Both tracks played together, as long as the longer one. Each is speech
/// at speaking level, so a plain sum rarely clips; what would is clamped.
pub fn mix_tracks(a: &[f32], b: &[f32]) -> Vec<f32> {
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut mixed = long.to_vec();
    for (out, &sample) in mixed.iter_mut().zip(short) {
        *out = (*out + sample).clamp(-1.0, 1.0);
    }
    mixed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::read_wav_samples;

    const RATE: u64 = WHISPER_SAMPLE_RATE as u64;

    fn seconds(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn samples_written_in_time_are_kept_back_to_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        let mut track = TrackWriter::create(&path).unwrap();
        let chunk = vec![0.5; 160];
        for i in 0..10 {
            track.write(&chunk, seconds(i as f64 * 0.01)).unwrap();
        }
        // Small jitter in the reported offset adds no silence.
        track.write(&chunk, seconds(0.2)).unwrap();
        assert_eq!(track.written(), 11 * 160);
        track.finalize().unwrap();

        let samples = read_wav_samples(&path).unwrap();
        assert_eq!(samples.len(), 11 * 160);
        assert!(samples.iter().all(|s| (s - 0.5).abs() < 0.001));
    }

    #[test]
    fn a_late_start_or_a_gap_becomes_silence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        let mut track = TrackWriter::create(&path).unwrap();
        // First audio a second into the meeting.
        track.write(&[0.5; 160], seconds(1.0)).unwrap();
        // Then a two-second gap.
        track.write(&[0.5; 160], seconds(3.0)).unwrap();
        track.finalize().unwrap();

        let samples = read_wav_samples(&path).unwrap();
        assert_eq!(samples.len() as u64, 3 * RATE + 160);
        assert!(samples[..RATE as usize].iter().all(|&s| s == 0.0));
        assert!((samples[RATE as usize] - 0.5).abs() < 0.001);
        assert!(samples[RATE as usize + 160..3 * RATE as usize]
            .iter()
            .all(|&s| s == 0.0));
    }

    #[test]
    fn a_track_that_was_never_finalized_is_still_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        let mut track = TrackWriter::create(&path).unwrap();
        track.write(&[0.25; 1600], Duration::ZERO).unwrap();
        track.writer.flush().unwrap();
        // A crash: the writer is never finalized.
        std::mem::forget(track);

        let samples = read_wav_samples(&path).unwrap();
        assert_eq!(samples.len(), 1600);
    }

    #[test]
    fn mixing_sums_aligned_samples_and_keeps_the_longer_tail() {
        let mixed = mix_tracks(&[0.25, 0.5, 0.75], &[0.25, 0.75]);
        assert_eq!(mixed, vec![0.5, 1.0, 0.75]);
        assert_eq!(mix_tracks(&[0.1], &[0.2, -0.3]), vec![0.1 + 0.2, -0.3]);
        assert!(mix_tracks(&[], &[]).is_empty());
    }
}
