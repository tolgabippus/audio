//! System audio for a meeting: what the default output device plays,
//! captured through cpal's loopback (an input stream on an output device;
//! a Core Audio process tap on macOS 14.2+), downmixed, resampled to 16 kHz
//! and written to a track on its own thread.
//!
//! The stream is rebuilt when it fails, when the default output device
//! changes, and while it has delivered only exact zeros: on macOS a tap
//! created before the user granted "System Audio Recording" stays silent
//! for its whole life, so a new one is needed once access is granted.

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use rodio::cpal::{
    self,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::track::TrackWriter;
use crate::audio_toolkit::audio::MonoResampler;

/// Callbacks of about 10 ms each: several seconds of slack for the writer.
const CHANNEL_CAPACITY: usize = 512;
/// How long the writer waits for audio before checking the stream's health.
const RECV_TIMEOUT: Duration = Duration::from_millis(200);
/// How often the default output device is compared with the captured one.
const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// Exact zeros for this long after a (re)build start a new stream.
const SILENT_REBUILD_AFTER: Duration = Duration::from_secs(4);
/// Rebuilds for silence stop after this many; nothing may be playing at all.
const MAX_SILENT_REBUILDS: u32 = 15;
/// After a failed build, how long until the next attempt.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(2);

/// One callback's worth of interleaved audio, and when it arrived.
struct Packet {
    at: Instant,
    samples: Vec<f32>,
}

/// What a finished capture reports.
pub struct SystemAudioOutcome {
    /// Any sample was not exactly zero.
    pub heard_sound: bool,
    /// The first error that kept system audio from being captured.
    pub error: Option<String>,
    pub dropped_packets: u64,
}

/// A running capture. `finish` stops it and closes the track.
pub struct SystemAudioCapture {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<SystemAudioOutcome>,
}

impl SystemAudioCapture {
    /// Starts capturing into a new track at `path`. `started` is the
    /// meeting's start, which the track is aligned to. Building the first
    /// stream happens on the capture thread: on macOS it can wait for the
    /// user to answer the permission prompt.
    pub fn start(path: PathBuf, started: Instant) -> Result<Self> {
        let track = TrackWriter::create(&path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new()
            .name("meeting-system-audio".into())
            .spawn({
                let stop = Arc::clone(&stop);
                move || Worker::new(track, started, stop).run()
            })?;
        Ok(Self { stop, thread })
    }

    pub fn finish(self) -> SystemAudioOutcome {
        self.stop.store(true, Ordering::Release);
        self.thread.join().unwrap_or_else(|_| SystemAudioOutcome {
            heard_sound: false,
            error: Some("The system audio thread crashed".into()),
            dropped_packets: 0,
        })
    }
}

/// An open loopback stream and what it delivers.
struct Open {
    stream: cpal::Stream,
    device_id: Option<cpal::DeviceId>,
    channels: usize,
    resampler: MonoResampler,
    built_at: Instant,
    failed: Arc<AtomicBool>,
}

struct Worker {
    track: TrackWriter,
    started: Instant,
    stop: Arc<AtomicBool>,
    tx: SyncSender<Packet>,
    rx: Receiver<Packet>,
    dropped: Arc<AtomicU64>,
    heard_sound: bool,
    silent_rebuilds: u32,
    error: Option<String>,
}

impl Worker {
    fn new(track: TrackWriter, started: Instant, stop: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::sync_channel(CHANNEL_CAPACITY);
        Self {
            track,
            started,
            stop,
            tx,
            rx,
            dropped: Arc::new(AtomicU64::new(0)),
            heard_sound: false,
            silent_rebuilds: 0,
            error: None,
        }
    }

    fn run(mut self) -> SystemAudioOutcome {
        let mut open = self.build();
        let mut last_device_check = Instant::now();
        let mut last_failure = Instant::now();

        while !self.stop.load(Ordering::Acquire) {
            match self.rx.recv_timeout(RECV_TIMEOUT) {
                Ok(packet) => {
                    if let Some(current) = open.as_mut() {
                        self.push(&mut current.resampler, current.channels, packet);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }

            let rebuild = match &open {
                None => last_failure.elapsed() >= RETRY_AFTER_FAILURE,
                Some(current) if current.failed.load(Ordering::Acquire) => {
                    warn!("System audio stream failed; rebuilding it");
                    true
                }
                Some(current)
                    if !self.heard_sound
                        && self.silent_rebuilds < MAX_SILENT_REBUILDS
                        && current.built_at.elapsed() >= SILENT_REBUILD_AFTER =>
                {
                    self.silent_rebuilds += 1;
                    debug!(
                        "System audio silent so far; rebuilding the stream ({}/{})",
                        self.silent_rebuilds, MAX_SILENT_REBUILDS
                    );
                    true
                }
                Some(current) if last_device_check.elapsed() >= DEVICE_CHECK_INTERVAL => {
                    last_device_check = Instant::now();
                    let changed = default_output_id() != current.device_id;
                    if changed {
                        info!("Default output device changed; following it");
                    }
                    changed
                }
                Some(_) => false,
            };
            if rebuild {
                if let Some(old) = open.take() {
                    self.close(old);
                }
                open = self.build();
                if open.is_none() {
                    last_failure = Instant::now();
                }
            }
        }

        if let Some(old) = open.take() {
            self.close(old);
        }
        let Self {
            track,
            heard_sound,
            mut error,
            dropped,
            ..
        } = self;
        if let Err(e) = track.finalize() {
            error.get_or_insert_with(|| format!("Failed to save system audio: {e}"));
        }
        SystemAudioOutcome {
            heard_sound,
            error,
            dropped_packets: dropped.load(Ordering::Relaxed),
        }
    }

    fn build(&mut self) -> Option<Open> {
        match open_loopback(self.tx.clone(), Arc::clone(&self.dropped)) {
            Ok(open) => Some(open),
            Err(e) => {
                warn!("Failed to capture system audio: {e}");
                self.record_error(e);
                None
            }
        }
    }

    /// Stops the stream and writes out what is still on its way.
    fn close(&mut self, open: Open) {
        let Open {
            stream,
            channels,
            mut resampler,
            ..
        } = open;
        drop(stream);
        while let Ok(packet) = self.rx.try_recv() {
            self.push(&mut resampler, channels, packet);
        }
        // The resampler's held-back delay follows straight on; a stream
        // built later fills any gap before its own first packet.
        match resampler.finish() {
            Ok(rest) => self.append(&rest, Duration::ZERO),
            Err(e) => self.record_error(e),
        }
    }

    fn push(&mut self, resampler: &mut MonoResampler, channels: usize, packet: Packet) {
        let frames = packet.samples.len() / channels.max(1);
        if !self.heard_sound && packet.samples.iter().any(|&s| s != 0.0) {
            info!("System audio is coming in");
            self.heard_sound = true;
        }
        let mono = packet
            .samples
            .chunks_exact(channels.max(1))
            .map(|frame| frame.iter().sum::<f32>() / channels as f32);
        if let Err(e) = resampler.push(mono) {
            self.record_error(e);
            return;
        }
        let output = resampler.take_output();
        // The packet's first frame was captured one packet before it arrived;
        // the resampler's delay is within the track's gap tolerance.
        let packet_duration = Duration::from_secs_f64(frames as f64 / resampler.rate() as f64);
        let offset = packet
            .at
            .saturating_duration_since(self.started)
            .saturating_sub(packet_duration);
        self.append(&output, offset);
    }

    fn append(&mut self, samples: &[f32], offset: Duration) {
        if let Err(e) = self.track.write(samples, offset) {
            self.record_error(anyhow!("Failed to save system audio: {e}"));
        }
    }

    fn record_error(&mut self, error: anyhow::Error) {
        self.error.get_or_insert_with(|| error.to_string());
    }
}

fn default_output_id() -> Option<cpal::DeviceId> {
    crate::audio_toolkit::get_cpal_host()
        .default_output_device()
        .and_then(|device| device.id().ok())
}

/// A loopback input stream on the default output device, playing.
fn open_loopback(tx: SyncSender<Packet>, dropped: Arc<AtomicU64>) -> Result<Open> {
    let device = crate::audio_toolkit::get_cpal_host()
        .default_output_device()
        .ok_or_else(|| anyhow!("No output device to capture"))?;
    let supported = device
        .default_output_config()
        .map_err(|e| anyhow!("Failed to read the output format: {e}"))?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(anyhow!(
            "Unsupported output sample format {:?}",
            supported.sample_format()
        ));
    }
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels as usize;
    let resampler = MonoResampler::new(config.sample_rate)?;
    let failed = Arc::new(AtomicBool::new(false));

    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let packet = Packet {
                    at: Instant::now(),
                    samples: data.to_vec(),
                };
                if let Err(TrySendError::Full(_)) = tx.try_send(packet) {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            {
                let failed = Arc::clone(&failed);
                move |e| {
                    warn!("System audio stream error: {e}");
                    failed.store(true, Ordering::Release);
                }
            },
            None,
        )
        .map_err(|e| anyhow!("Failed to open system audio capture: {e}"))?;
    stream
        .play()
        .map_err(|e| anyhow!("Failed to start system audio capture: {e}"))?;

    let name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_default();
    info!(
        "Capturing system audio from '{}' ({} Hz, {} channels)",
        name, config.sample_rate, channels
    );
    Ok(Open {
        stream,
        device_id: device.id().ok(),
        channels,
        resampler,
        built_at: Instant::now(),
        failed,
    })
}
