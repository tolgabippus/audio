//! Meeting recording: the microphone ("you") and the system audio ("the
//! others") at once, each written to its own track as it arrives. Stopping
//! mixes the tracks into one recording and queues it as a file
//! transcription, so the result lands in History.
//!
//! Tracks live in `<app data>/meetings/<id>/` until their recording exists;
//! a meeting cut short by a crash is imported on the next start. They are
//! kept after that for transcribing each side on its own, and removed with
//! their History entry.

mod system_audio;
mod track;
mod transcript;

use anyhow::{anyhow, Result};
use handy_recorder::{AudioChunk, EndReason, Recorder, RecorderConfig, Sink};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::audio_toolkit::{read_wav_samples, save_wav_file};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::{HistoryEntry, HistoryManager};
use crate::managers::transcription::{
    ChunkControl, TimedText, Transcription, TranscriptionManager,
};
use crate::managers::transcription_jobs::TranscriptionJobManager;
use crate::notices::{self, Notice, NoticeCode, NoticeSource};
use system_audio::{SystemAudioCapture, SystemAudioOutcome};
use track::{mix_tracks, TrackWriter};
use transcript::{drop_echo, format_transcript, merge_turns, SpeakerLabels};

const MEETINGS_DIR: &str = "meetings";
const MIC_TRACK: &str = "mic.wav";
const SYSTEM_TRACK: &str = "system.wav";
/// The turns of the last transcription, for showing and exporting a meeting.
const TRANSCRIPT_FILE: &str = "transcript.json";
/// A meeting's recording is `handy-meeting-<id>.wav`; its tracks are in
/// `meetings/<id>/`.
const RECORDING_PREFIX: &str = "handy-meeting-";

/// Whether a meeting is being recorded, and since when.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event,
)]
pub struct MeetingState {
    pub recording: bool,
    /// Unix milliseconds; set while recording.
    pub started_at_ms: Option<i64>,
}

struct Active {
    id: String,
    dir: PathBuf,
    started_at_ms: i64,
    /// Why the microphone isn't recording, when it isn't.
    microphone: Result<Recorder<MicSink>, String>,
    system: SystemAudioCapture,
}

pub struct MeetingManager {
    app: AppHandle,
    active: Mutex<Option<Active>>,
}

impl MeetingManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app: app.clone(),
            active: Mutex::new(None),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Option<Active>> {
        self.active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn is_recording(&self) -> bool {
        self.lock().is_some()
    }

    pub fn state(&self) -> MeetingState {
        match self.lock().as_ref() {
            Some(active) => MeetingState {
                recording: true,
                started_at_ms: Some(active.started_at_ms),
            },
            None => MeetingState::default(),
        }
    }

    fn meetings_dir(&self) -> Result<PathBuf> {
        Ok(crate::portable::app_data_dir(&self.app)?.join(MEETINGS_DIR))
    }

    /// Starts recording. The microphone failing to open is only a warning:
    /// the system audio still makes a useful recording.
    pub fn start(&self) -> Result<MeetingState> {
        let mut active = self.lock();
        if active.is_some() {
            return Err(anyhow!("A meeting is already being recorded"));
        }
        let started = Instant::now();
        let started_at_ms = chrono::Utc::now().timestamp_millis();
        let id = started_at_ms.to_string();
        let dir = self.meetings_dir()?.join(&id);
        std::fs::create_dir_all(&dir)?;

        let system = SystemAudioCapture::start(dir.join(SYSTEM_TRACK), started)?;
        let microphone = self
            .open_microphone(&dir, started)
            .map_err(|e| e.to_string());
        let microphone_problem = microphone.as_ref().err().cloned();

        info!("Started meeting recording {id}");
        *active = Some(Active {
            id,
            dir,
            started_at_ms,
            microphone,
            system,
        });
        // Raising a notice rebuilds the tray, which reads this lock.
        drop(active);
        if let Some(problem) = microphone_problem {
            self.warn(NoticeCode::MeetingNoMicrophone, problem);
        }
        self.changed();
        Ok(self.state())
    }

    fn open_microphone(&self, dir: &Path, started: Instant) -> Result<Recorder<MicSink>> {
        let device = self
            .app
            .state::<Arc<AudioRecordingManager>>()
            .selected_microphone_device();
        let recorder = Recorder::open(RecorderConfig {
            device,
            ..RecorderConfig::speech()
        })?;
        let sink = MicSink {
            track: TrackWriter::create(&dir.join(MIC_TRACK))?,
            started,
            error: None,
        };
        recorder
            .start(sink)
            .map_err(|e| anyhow!("Failed to start the microphone: {}", e.error))?;
        Ok(recorder)
    }

    /// Stops recording and hands the meeting to History. Mixing a long
    /// meeting takes a moment, so callers run this off the UI thread.
    pub fn stop(&self) -> Result<HistoryEntry> {
        let active = self
            .lock()
            .take()
            .ok_or_else(|| anyhow!("No meeting is being recorded"))?;
        self.changed();
        info!("Stopping meeting recording {}", active.id);

        let microphone_problem = match &active.microphone {
            Ok(recorder) => stop_microphone(recorder),
            Err(problem) => Some(problem.clone()),
        };
        let system = active.system.finish();
        if system.dropped_packets > 0 {
            warn!(
                "System audio lost {} packets during the meeting",
                system.dropped_packets
            );
        }

        let result = self.import(&active.id, &active.dir);
        match &result {
            Ok(_) => {
                // Clear first, so a warning about this meeting survives it.
                notices::clear(&self.app, NoticeSource::Meeting);
                if let Some(problem) = system_problem(&system) {
                    self.warn(NoticeCode::MeetingNoSystemAudio, problem);
                } else if let Some(problem) = microphone_problem {
                    self.warn(NoticeCode::MeetingNoMicrophone, problem);
                }
            }
            Err(e) => {
                error!("Failed to save meeting {}: {e}", active.id);
                notices::raise(
                    &self.app,
                    Notice::new(NoticeCode::MeetingRecordingFailed, NoticeSource::Meeting)
                        .with_detail(e.to_string()),
                );
            }
        }
        result
    }

    fn warn(&self, code: NoticeCode, detail: String) {
        warn!("Meeting recording: {detail}");
        notices::raise(
            &self.app,
            Notice::new(code, NoticeSource::Meeting).with_detail(detail),
        );
    }

    /// Starts recording, or stops a recording on its own thread.
    pub fn toggle(self: &Arc<Self>) {
        if !self.is_recording() {
            if let Err(e) = self.start() {
                error!("Failed to start meeting recording: {e}");
                notices::raise(
                    &self.app,
                    Notice::new(NoticeCode::MeetingRecordingFailed, NoticeSource::Meeting)
                        .with_detail(e.to_string()),
                );
            }
            return;
        }
        let manager = Arc::clone(self);
        std::thread::spawn(move || {
            if let Err(e) = manager.stop() {
                error!("Failed to stop meeting recording: {e}");
            }
        });
    }

    /// Mixes the meeting's tracks into its recording and queues it.
    fn import(&self, id: &str, dir: &Path) -> Result<HistoryEntry> {
        let read = |name: &str| {
            let path = dir.join(name);
            if path.exists() {
                read_wav_samples(&path).map_err(|e| anyhow!("Failed to read {name}: {e}"))
            } else {
                Ok(Vec::new())
            }
        };
        let mixed = mix_tracks(&read(MIC_TRACK)?, &read(SYSTEM_TRACK)?);
        if mixed.is_empty() {
            return Err(anyhow!("The meeting recorded no audio"));
        }

        let hm = self.app.state::<Arc<HistoryManager>>();
        let file_name = recording_file_name(id);
        let recording = hm.get_audio_file_path(&file_name);
        // Written aside and renamed: the recording's existence is what marks
        // the meeting as imported.
        let partial = recording.with_extension("wav.partial");
        save_wav_file(&partial, &mixed)?;
        std::fs::rename(&partial, &recording)?;
        drop(mixed);

        self.app
            .state::<Arc<TranscriptionJobManager>>()
            .transcribe_recording(file_name, source_name(id))
    }

    /// Imports meetings a crash or quit cut short. Runs once at startup.
    pub fn recover_unfinished(&self) {
        let Ok(dir) = self.meetings_dir() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let hm = self.app.state::<Arc<HistoryManager>>();
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let recording_now = self.lock().as_ref().is_some_and(|active| active.id == id);
            if recording_now
                || !path.is_dir()
                || hm.get_audio_file_path(&recording_file_name(&id)).exists()
            {
                continue;
            }
            match self.import(&id, &path) {
                Ok(entry) => info!(
                    "Recovered unfinished meeting {id} as history entry {}",
                    entry.id
                ),
                Err(e) => warn!("Failed to recover unfinished meeting {id}: {e}"),
            }
        }
    }

    fn changed(&self) {
        if let Err(e) = self.state().emit(&self.app) {
            error!("Failed to emit meeting state: {e}");
        }
        crate::tray::update_tray_menu(&self.app);
    }
}

/// Stops the meeting microphone and closes its track; what went wrong, if
/// anything did.
fn stop_microphone(recorder: &Recorder<MicSink>) -> Option<String> {
    let stopped = match recorder.stop() {
        Ok(stopped) => stopped,
        Err(e) => return Some(format!("Failed to stop the microphone: {e}")),
    };
    let ended = match &stopped.end_reason {
        EndReason::StopCalled => None,
        EndReason::RecorderFailed(e) => Some(format!("The microphone stopped: {e}")),
        EndReason::SinkPanicked(message) => Some(format!("The microphone crashed: {message}")),
    };
    let MicSink { track, error, .. } = stopped.sink;
    let saved = track
        .finalize()
        .err()
        .map(|e| format!("Failed to save the microphone: {e}"));
    ended.or(error).or(saved)
}

/// Why the meeting has no system audio, if it doesn't.
fn system_problem(outcome: &SystemAudioOutcome) -> Option<String> {
    if outcome.heard_sound {
        return None;
    }
    Some(outcome.error.clone().unwrap_or_else(|| {
        "Only silence came in. On macOS, check System Audio Recording for Handy in \
         Privacy & Security."
            .into()
    }))
}

fn recording_file_name(id: &str) -> String {
    format!("{RECORDING_PREFIX}{id}.wav")
}

/// What History shows for a meeting: its local start time.
fn source_name(id: &str) -> String {
    let started = id
        .parse::<i64>()
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|utc| {
            utc.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| id.to_string());
    format!("Meeting {started}")
}

/// The folder of the meeting whose recording is `file_name`, if it is one.
fn tracks_dir(app: &AppHandle, file_name: &str) -> Option<PathBuf> {
    let id = file_name
        .strip_prefix(RECORDING_PREFIX)
        .and_then(|rest| rest.strip_suffix(".wav"))?;
    let dir = crate::portable::app_data_dir(app)
        .ok()?
        .join(MEETINGS_DIR)
        .join(id);
    dir.is_dir().then_some(dir)
}

/// Removes a meeting's tracks along with its History entry.
pub fn remove_tracks(app: &AppHandle, file_name: &str) {
    let Some(dir) = tracks_dir(app, file_name) else {
        return;
    };
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        if e.kind() != std::io::ErrorKind::NotFound {
            error!("Failed to remove meeting tracks {}: {e}", dir.display());
        }
    }
}

/// Transcribes a meeting from its two tracks, so each side keeps its words
/// and the transcript says who spoke when. `None` when the recording isn't a
/// meeting with tracks (transcribe its recording instead), `Some(None)` when
/// `control` stopped it.
pub fn transcribe_tracks(
    app: &AppHandle,
    file_name: &str,
    control: &dyn ChunkControl,
) -> Option<Result<Option<Transcription>>> {
    let dir = tracks_dir(app, file_name)?;
    Some(transcribe_dir(app, &dir, control))
}

fn transcribe_dir(
    app: &AppHandle,
    dir: &Path,
    control: &dyn ChunkControl,
) -> Result<Option<Transcription>> {
    let read = |name: &str| -> Result<Vec<f32>> {
        let path = dir.join(name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_wav_samples(&path).map_err(|e| anyhow!("Failed to load {name}: {e}"))
    };
    let microphone = read(MIC_TRACK)?;
    let system = read(SYSTEM_TRACK)?;
    let total = microphone.len() + system.len();
    if total == 0 {
        return Err(anyhow!("Recording has no audio samples"));
    }

    let tm = app.state::<Arc<TranscriptionManager>>();
    let mic_len = microphone.len();
    let Some(mic) = tm.transcribe_in_chunks_timed(
        microphone,
        &Share {
            inner: control,
            before: 0,
            total,
        },
    )?
    else {
        return Ok(None);
    };
    let Some(others) = tm.transcribe_in_chunks_timed(
        system,
        &Share {
            inner: control,
            before: mic_len,
            total,
        },
    )?
    else {
        return Ok(None);
    };

    // The side that said more decides the language History shows.
    let language = if spoken_chars(&mic.texts) >= spoken_chars(&others.texts) {
        mic.language.or(others.language)
    } else {
        others.language.or(mic.language)
    };
    let turns = merge_turns(drop_echo(mic.texts, &others.texts), others.texts);
    if let Err(e) = serde_json::to_vec_pretty(&turns)
        .map_err(anyhow::Error::from)
        .and_then(|json| std::fs::write(dir.join(TRANSCRIPT_FILE), json).map_err(Into::into))
    {
        warn!("Failed to save the meeting's turns: {e}");
    }

    let strings = crate::tray_i18n::get_tray_translations(Some(
        crate::settings::get_settings(app).app_language,
    ));
    let labels = SpeakerLabels {
        you: &strings.meeting_speaker_you,
        others: &strings.meeting_speaker_others,
    };
    Ok(Some(Transcription {
        text: format_transcript(&turns, &labels),
        language,
    }))
}

fn spoken_chars(texts: &[TimedText]) -> usize {
    texts.iter().map(|t| t.text.len()).sum()
}

/// One track's part of a meeting's progress: the microphone first, then the
/// system audio, counted together.
struct Share<'a> {
    inner: &'a dyn ChunkControl,
    before: usize,
    total: usize,
}

impl ChunkControl for Share<'_> {
    fn proceed(&self) -> bool {
        self.inner.proceed()
    }

    fn progress(&self, done: usize, _total: usize) {
        self.inner.progress(self.before + done, self.total);
    }
}

/// Writes the meeting microphone to its track.
struct MicSink {
    track: TrackWriter,
    started: Instant,
    /// The first write error; later audio is dropped.
    error: Option<String>,
}

impl Sink for MicSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        if self.error.is_some() {
            return;
        }
        let real = &chunk.samples[..chunk.valid_frames * chunk.channels as usize];
        let duration =
            Duration::from_secs_f64(chunk.valid_frames as f64 / chunk.sample_rate as f64);
        let offset = self.started.elapsed().saturating_sub(duration);
        if let Err(e) = self.track.write(real, offset) {
            self.error = Some(format!("Failed to save the microphone: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meeting_recording_maps_back_to_its_tracks() {
        let file_name = recording_file_name("1730740000000");
        assert_eq!(file_name, "handy-meeting-1730740000000.wav");
        assert_eq!(
            file_name
                .strip_prefix(RECORDING_PREFIX)
                .and_then(|rest| rest.strip_suffix(".wav")),
            Some("1730740000000")
        );
    }

    #[test]
    fn history_names_a_meeting_by_its_start() {
        assert!(source_name("1730740000000").starts_with("Meeting 2024-11-0"));
        assert_eq!(source_name("not-a-time"), "Meeting not-a-time");
    }

    #[test]
    fn silent_system_audio_is_reported_with_its_reason() {
        let heard = SystemAudioOutcome {
            heard_sound: true,
            error: Some("ignored".into()),
            dropped_packets: 0,
        };
        assert_eq!(system_problem(&heard), None);
        let failed = SystemAudioOutcome {
            heard_sound: false,
            error: Some("no device".into()),
            dropped_packets: 0,
        };
        assert_eq!(system_problem(&failed).as_deref(), Some("no device"));
        let silent = SystemAudioOutcome {
            heard_sound: false,
            error: None,
            dropped_packets: 0,
        };
        assert!(system_problem(&silent)
            .unwrap()
            .contains("System Audio Recording"));
    }
}
