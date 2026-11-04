//! Mono audio at any rate in, 16 kHz out, one resampler step at a time: for
//! a file being decoded, or a live stream written out as it arrives.

use anyhow::{anyhow, Result};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;

/// Frames the resampler takes per step.
const RESAMPLER_CHUNK_FRAMES: usize = 1024;

pub struct MonoResampler {
    rate: u32,
    /// Input frames taken in so far.
    input_len: usize,
    /// `None` when the input is already at 16 kHz.
    stage: Option<Stage>,
    output: Vec<f32>,
    /// Output frames already handed out by `take_output`.
    taken_len: usize,
}

struct Stage {
    resampler: Fft<f32>,
    /// Input not yet resampled: less than one step, plus the latest push.
    pending: Vec<f32>,
    /// One step's output.
    scratch: Vec<f32>,
    /// Output frames of the resampler's delay still to drop.
    delay_left: usize,
}

impl MonoResampler {
    pub fn new(rate: u32) -> Result<Self> {
        let target = WHISPER_SAMPLE_RATE as usize;
        let stage = if rate as usize == target {
            None
        } else {
            let resampler = Fft::<f32>::new(
                rate as usize,
                target,
                RESAMPLER_CHUNK_FRAMES,
                1,
                FixedSync::Input,
            )
            .map_err(|e| anyhow!("Failed to set up resampling from {} Hz: {}", rate, e))?;
            Some(Stage {
                pending: Vec::with_capacity(2 * resampler.input_frames_max()),
                scratch: vec![0.0; resampler.output_frames_max()],
                delay_left: resampler.output_delay(),
                resampler,
            })
        };
        Ok(Self {
            rate,
            input_len: 0,
            stage,
            output: Vec::new(),
            taken_len: 0,
        })
    }

    /// The input's sample rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Input frames taken in so far.
    pub fn input_len(&self) -> usize {
        self.input_len
    }

    /// The 16 kHz length of `input_len` frames at the input rate.
    fn expected_len(&self, input_len: usize) -> usize {
        match &self.stage {
            Some(stage) => (stage.resampler.resample_ratio() * input_len as f64).ceil() as usize,
            None => input_len,
        }
    }

    pub fn push(&mut self, samples: impl Iterator<Item = f32>) -> Result<()> {
        let Some(stage) = self.stage.as_mut() else {
            let before = self.output.len();
            self.output.extend(samples);
            self.input_len += self.output.len() - before;
            return Ok(());
        };
        let before = stage.pending.len();
        stage.pending.extend(samples);
        self.input_len += stage.pending.len() - before;
        let mut taken = 0;
        while stage.pending.len() - taken >= stage.resampler.input_frames_next() {
            taken += stage.step(taken, None, &mut self.output)?;
        }
        stage.pending.drain(..taken);
        Ok(())
    }

    /// The 16 kHz audio produced since the last call. The resampler holds
    /// back its delay's worth until `finish`.
    pub fn take_output(&mut self) -> Vec<f32> {
        let output = std::mem::take(&mut self.output);
        self.taken_len += output.len();
        output
    }

    /// The rest of the 16 kHz audio, so that everything taken adds up to
    /// the input's length at 16 kHz.
    pub fn finish(mut self) -> Result<Vec<f32>> {
        let expected = self
            .expected_len(self.input_len)
            .saturating_sub(self.taken_len);
        let Some(mut stage) = self.stage.take() else {
            return Ok(self.output);
        };
        let left = stage.pending.len();
        if left > 0 {
            stage.step(0, Some(left), &mut self.output)?;
        }
        // The resampler holds back its delay's worth; silence pushes it out.
        while self.output.len() < expected {
            stage.step(0, Some(0), &mut self.output)?;
        }
        self.output.truncate(expected);
        Ok(self.output)
    }
}

impl Stage {
    /// Resamples one step of `pending` from `offset` (or only `partial` of
    /// it, padded with silence) into `output`. Returns the frames it took.
    fn step(
        &mut self,
        offset: usize,
        partial: Option<usize>,
        output: &mut Vec<f32>,
    ) -> Result<usize> {
        let input = InterleavedSlice::new(&self.pending[..], 1, self.pending.len())
            .map_err(|e| anyhow!("Failed to resample: {}", e))?;
        let scratch_len = self.scratch.len();
        let mut out = InterleavedSlice::new_mut(&mut self.scratch[..], 1, scratch_len)
            .map_err(|e| anyhow!("Failed to resample: {}", e))?;
        let mut indexing = Indexing::new().input_offset(offset);
        indexing.partial_len = partial;
        let (taken, produced) = self
            .resampler
            .process_into_buffer(&input, &mut out, Some(&indexing))
            .map_err(|e| anyhow!("Failed to resample: {}", e))?;
        let dropped = self.delay_left.min(produced);
        self.delay_left -= dropped;
        output.extend_from_slice(&self.scratch[dropped..produced]);
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resample_in_packets(mono: &[f32], rate: u32, packet: usize) -> Vec<f32> {
        let mut resampled = MonoResampler::new(rate).unwrap();
        for chunk in mono.chunks(packet) {
            resampled.push(chunk.iter().copied()).unwrap();
        }
        resampled.finish().unwrap()
    }

    fn tone(rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| (i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect()
    }

    #[test]
    fn resampling_keeps_length_and_timing_whatever_the_packet_size() {
        for rate in [16_000, 44_100, 48_000] {
            for frames in [500, rate as usize + 337] {
                let mono = tone(rate, frames);
                let whole = resample_in_packets(&mono, rate, frames);
                let expected_len = (frames as f64 * 16_000.0 / rate as f64).ceil() as usize;
                assert_eq!(whole.len(), expected_len, "{} Hz, {} frames", rate, frames);
                for packet in [333, 4096] {
                    assert_eq!(resample_in_packets(&mono, rate, packet), whole);
                }
                // Delay trimmed: within half a sample of the 16 kHz tone.
                let reference = tone(16_000, whole.len());
                let interior = 100..whole.len().saturating_sub(100);
                let max_error = interior
                    .map(|i| (whole[i] - reference[i]).abs())
                    .fold(0.0f32, f32::max);
                assert!(max_error < 0.04, "{} Hz: error {}", rate, max_error);
            }
        }
    }

    #[test]
    fn output_taken_along_the_way_adds_up_to_the_whole() {
        for rate in [16_000, 48_000] {
            let mono = tone(rate, 3 * rate as usize + 123);
            let whole = resample_in_packets(&mono, rate, mono.len());
            let mut resampler = MonoResampler::new(rate).unwrap();
            let mut streamed = Vec::new();
            for chunk in mono.chunks(480) {
                resampler.push(chunk.iter().copied()).unwrap();
                streamed.extend(resampler.take_output());
            }
            assert_eq!(resampler.input_len(), mono.len());
            streamed.extend(resampler.finish().unwrap());
            assert_eq!(streamed, whole, "{} Hz", rate);
        }
    }
}
