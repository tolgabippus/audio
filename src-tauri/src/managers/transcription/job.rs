//! Background transcription, always chunked: chunks are the unit of
//! progress, of yielding to dictation, and of retry.

use anyhow::Result;
use log::{debug, info, warn};
use std::time::Instant;
use transcribe_cpp::Transcript;

use super::{
    agreed_language, engine_error, post_process_transcription_text, real_time_factor, BatchContext,
    RunSetup, Transcription, TranscriptionManager,
};
use crate::audio_toolkit::chunking::Chunker;
use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;
use crate::engine_supervisor::EngineError;
use crate::language_detection::LanguageDecision;
use crate::managers::audio::create_detector;
use crate::settings::AppSettings;

/// Tries per chunk when a dictation's cancel interrupts it.
const MAX_CHUNK_ATTEMPTS: usize = 3;

/// Audio handed to the chunker at a time: the first chunk is transcribed
/// without scoring the whole file first. Cuts match the whole-file plan (a
/// rare short blip at a feed boundary can nudge one within its pause).
const FEED_SAMPLES: usize = 60 * 16_000;

const MS_PER_SECOND: u64 = 1000;

/// What a background transcription asks of whoever runs it.
pub trait ChunkControl {
    /// Called before each feed to the chunker and before each chunk is
    /// decoded. Blocks while dictation is using the engine; `false` stops the
    /// transcription.
    fn proceed(&self) -> bool;
    /// `done` of `total` samples are transcribed.
    fn progress(&self, done: usize, total: usize);
}

/// One stretch of speech, with its time in the audio it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedText {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// A transcription kept as timed stretches rather than one text.
pub struct TimedTranscription {
    pub texts: Vec<TimedText>,
    /// The language it ran in and how that was chosen.
    pub language: Option<LanguageDecision>,
}

/// One decoded chunk and where it sits in the audio, in samples.
struct DecodedChunk {
    offset: usize,
    len: usize,
    transcript: Transcript,
}

/// Every chunk of a background transcription, with how its runs were set up.
struct DecodedChunks {
    chunks: Vec<DecodedChunk>,
    settings: AppSettings,
    setup: RunSetup,
}

impl DecodedChunks {
    /// A chunk's text, cleaned up like any transcription.
    fn post_process(&self, text: String, language: Option<String>) -> String {
        let output_language = self.setup.output_language(&self.settings, language);
        post_process_transcription_text(
            text,
            &self.settings,
            self.setup.custom_words_prompted(),
            &output_language,
            &self.setup.languages,
        )
    }

    fn language(&self) -> LanguageDecision {
        let agreed = agreed_language(self.chunks.iter().map(|c| &c.transcript));
        self.setup.spoken_language(agreed.as_deref())
    }
}

impl TranscriptionManager {
    /// Transcribe `audio` chunk by chunk with the selected model, one
    /// paragraph per chunk. `None` if `control` stopped it.
    pub fn transcribe_in_chunks(
        &self,
        audio: Vec<f32>,
        control: &dyn ChunkControl,
    ) -> Result<Option<Transcription>> {
        if audio.is_empty() {
            return Ok(Some(Transcription {
                text: String::new(),
                language: None,
            }));
        }
        let Some(decoded) = self.decode_in_chunks(audio, control)? else {
            return Ok(None);
        };
        let language = decoded.language();
        let paragraphs: Vec<String> = decoded
            .chunks
            .iter()
            .map(|c| decoded.post_process(c.transcript.text.clone(), c.transcript.language.clone()))
            .filter(|p| !p.trim().is_empty())
            .collect();
        Ok(Some(Transcription {
            text: paragraphs.join("\n\n"),
            language: Some(language),
        }))
    }

    /// [`transcribe_in_chunks`](Self::transcribe_in_chunks), kept as timed
    /// stretches: the model's segments where it gives them, otherwise one
    /// stretch per chunk. Times are in milliseconds from the start of `audio`.
    pub fn transcribe_in_chunks_timed(
        &self,
        audio: Vec<f32>,
        control: &dyn ChunkControl,
    ) -> Result<Option<TimedTranscription>> {
        if audio.is_empty() {
            return Ok(Some(TimedTranscription {
                texts: Vec::new(),
                language: None,
            }));
        }
        let Some(decoded) = self.decode_in_chunks(audio, control)? else {
            return Ok(None);
        };
        let language = decoded.language();
        let mut texts = Vec::new();
        for chunk in &decoded.chunks {
            let offset_ms = samples_to_ms(chunk.offset);
            let language = chunk.transcript.language.clone();
            for (start_ms, end_ms, raw) in chunk_stretches(chunk) {
                let text = decoded.post_process(raw, language.clone());
                if !text.trim().is_empty() {
                    texts.push(TimedText {
                        start_ms: offset_ms + start_ms,
                        end_ms: offset_ms + end_ms,
                        text: text.trim().to_string(),
                    });
                }
            }
        }
        Ok(Some(TimedTranscription {
            texts,
            language: Some(language),
        }))
    }

    /// Cuts `audio` at pauses and decodes each chunk. `None` if `control`
    /// stopped it.
    fn decode_in_chunks(
        &self,
        audio: Vec<f32>,
        control: &dyn ChunkControl,
    ) -> Result<Option<DecodedChunks>> {
        let started = Instant::now();
        let total = audio.len();

        self.touch_activity();
        self.initiate_model_load(false);
        // Read once: the model can't change until the job ends.
        let BatchContext {
            settings,
            active_model,
            setup,
            ..
        } = self.batch_context(&audio)?;

        let backend = settings.vad_backend;
        let app_handle = self.app_handle.clone();
        let mut chunker = Chunker::new(Box::new(move || create_detector(backend, &app_handle)));
        let mut pieces = audio.chunks(FEED_SAMPLES);

        let mut decoded: Vec<DecodedChunk> = Vec::new();
        let mut done = 0;
        control.progress(done, total);
        loop {
            // Scoring waits out dictation too, and stops on cancel.
            if !control.proceed() {
                return Ok(None);
            }
            let (chunks, last) = match pieces.next() {
                Some(piece) => (chunker.push(piece), false),
                None => (chunker.finish(), true),
            };
            for chunk in chunks {
                let len = chunk.len();
                let mut attempts = 0;
                let transcript = loop {
                    if !control.proceed() {
                        return Ok(None);
                    }
                    attempts += 1;
                    // A fresh mark per chunk: a dictation's cancel stops only
                    // the chunk it caught, which is then tried again.
                    let mark = self.engine.cancel_mark();
                    match self.run_batch(mark, chunk.clone(), &setup.run_options, &active_model) {
                        Ok(transcript) => break transcript,
                        Err(EngineError::Cancelled) if attempts < MAX_CHUNK_ATTEMPTS => {
                            warn!("A background chunk was cancelled; trying it again");
                        }
                        Err(e) => return Err(engine_error(e)),
                    }
                };
                // Chunks are consecutive cuts of the audio.
                decoded.push(DecodedChunk {
                    offset: done,
                    len,
                    transcript,
                });
                done += len;
                control.progress(done, total);
            }
            if last {
                break;
            }
        }

        debug!("Transcribed {} chunks in the background", decoded.len());
        let elapsed_secs = started.elapsed().as_secs_f64();
        let audio_secs = total as f64 / 16_000.0;
        info!(
            "Background transcription of {:.1}s of audio in {} chunks took {:.1}s ({:.1}x real time)",
            audio_secs,
            decoded.len(),
            elapsed_secs,
            real_time_factor(audio_secs, elapsed_secs)
        );
        Ok(Some(DecodedChunks {
            chunks: decoded,
            settings,
            setup,
        }))
    }
}

fn samples_to_ms(samples: usize) -> u64 {
    (samples as u64 * MS_PER_SECOND) / WHISPER_SAMPLE_RATE as u64
}

/// A chunk's `(start_ms, end_ms, text)` stretches relative to the chunk: its
/// timed segments, or the whole chunk when the model gave none.
fn chunk_stretches(chunk: &DecodedChunk) -> Vec<(u64, u64, String)> {
    let timed: Vec<(u64, u64, String)> = chunk
        .transcript
        .segments
        .iter()
        .filter(|s| s.t1_ms > s.t0_ms && !s.text.trim().is_empty())
        .map(|s| (s.t0_ms.max(0) as u64, s.t1_ms as u64, s.text.clone()))
        .collect();
    if timed.is_empty() {
        vec![(0, samples_to_ms(chunk.len), chunk.transcript.text.clone())]
    } else {
        timed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcribe_cpp::Segment;

    fn chunk(len: usize, text: &str, segments: Vec<Segment>) -> DecodedChunk {
        DecodedChunk {
            offset: 0,
            len,
            transcript: Transcript {
                text: text.to_string(),
                segments,
                ..Default::default()
            },
        }
    }

    fn segment(t0_ms: i64, t1_ms: i64, text: &str) -> Segment {
        Segment {
            t0_ms,
            t1_ms,
            text: text.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_chunk_keeps_its_timed_segments() {
        let stretches = chunk_stretches(&chunk(
            16_000 * 10,
            "Erstens. Zweitens.",
            vec![
                segment(500, 2_000, "Erstens."),
                segment(3_000, 4_500, "Zweitens."),
            ],
        ));
        assert_eq!(
            stretches,
            vec![
                (500, 2_000, "Erstens.".to_string()),
                (3_000, 4_500, "Zweitens.".to_string())
            ]
        );
    }

    #[test]
    fn a_chunk_without_timestamps_is_one_stretch() {
        // Untimed rows, as speaker-attributed or untimed models return them.
        let stretches = chunk_stretches(&chunk(
            16_000 * 30,
            "Alles in einem.",
            vec![segment(0, 0, "Alles in einem.")],
        ));
        assert_eq!(stretches, vec![(0, 30_000, "Alles in einem.".to_string())]);
    }

    #[test]
    fn samples_convert_to_milliseconds_at_16_khz() {
        assert_eq!(samples_to_ms(16_000), 1_000);
        assert_eq!(samples_to_ms(8), 0);
        assert_eq!(samples_to_ms(16_000 * 3_600), 3_600_000);
    }
}
