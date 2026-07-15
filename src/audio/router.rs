use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::pedals::{MetronomePedal, PedalChain, PedalSpec};
use crate::project::{DrumLane, DrumRole, DrumSequence};

const STREAM_BUFFER_FRAMES: usize = 96;
const LIVE_BUFFER_TARGET_FRAMES: usize = 256;
const LIVE_BUFFER_CAPACITY_FRAMES: usize = 2048;

#[derive(Default, Clone, Copy)]
pub struct Meters {
    pub input_peak: f32,
    pub output_peak: f32,
}

#[derive(Clone)]
pub struct AudioSample {
    pub samples: Arc<[f32]>,
    pub sample_rate_hz: u32,
}

#[derive(Clone)]
pub struct DrumSampleKit {
    pub kick: Vec<Vec<Arc<AudioSample>>>,
    pub snare: Vec<Vec<Arc<AudioSample>>>,
    pub closed_hat: Vec<Vec<Arc<AudioSample>>>,
    pub open_hat: Vec<Vec<Arc<AudioSample>>>,
    pub pedal_hat: Vec<Vec<Arc<AudioSample>>>,
    pub high_tom: Vec<Vec<Arc<AudioSample>>>,
    pub low_tom: Vec<Vec<Arc<AudioSample>>>,
    pub ride: Vec<Vec<Arc<AudioSample>>>,
    pub crash: Vec<Vec<Arc<AudioSample>>>,
    pub percussion: Vec<Vec<Arc<AudioSample>>>,
}

impl DrumSampleKit {
    fn sample_for_lane(&self, lane_index: usize, lane: &DrumLane, variant: usize, velocity: f32) -> Arc<AudioSample> {
        let layers = match lane.effective_role() {
            DrumRole::Kick => &self.kick,
            DrumRole::Snare => &self.snare,
            DrumRole::OpenHat => &self.open_hat,
            DrumRole::ClosedHat => &self.closed_hat,
            DrumRole::PedalHat => &self.pedal_hat,
            DrumRole::HighTom | DrumRole::MidTom => &self.high_tom,
            DrumRole::FloorTom => &self.low_tom,
            DrumRole::Ride => &self.ride,
            DrumRole::Crash => &self.crash,
            DrumRole::Percussion => &self.percussion,
            DrumRole::Other if lane_index == 0 => &self.kick,
            DrumRole::Other if lane_index == 1 => &self.snare,
            _ => &self.closed_hat,
        };
        let index = ((layers.len().saturating_sub(1)) as f32 * velocity.clamp(0.0, 1.0)).round() as usize;
        let recordings = &layers[index.min(layers.len().saturating_sub(1))];
        Arc::clone(&recordings[variant % recordings.len().max(1)])
    }
}

#[derive(Clone)]
pub struct PlaybackDrumClip {
    pub active: bool,
    pub start_beat: f64,
    pub length_beats: f64,
    pub source_offset_beats: f64,
    pub loop_count: f64,
    pub volume: f32,
    pub sequence: DrumSequence,
}

#[derive(Clone)]
pub struct SequencerPreview {
    pub playing: bool,
    pub sequence: DrumSequence,
    pub playhead_beats: f64,
    pub audition_lanes: Vec<usize>,
    pub audition_nonce: u64,
    pub restart_nonce: u64,
}

#[derive(Clone)]
pub struct SamplePreview {
    pub sample: Option<Arc<AudioSample>>,
    pub nonce: u64,
}

#[derive(Clone)]
pub struct PlaybackClip {
    pub start_beat: f64,
    pub length_beats: f64,
    pub source_offset_beats: f64,
    pub loop_count: f64,
    pub sample_rate_hz: u32,
    pub volume: f32,
    pub samples: Arc<[f32]>,
}

#[derive(Clone)]
pub struct PlaybackState {
    pub playing: bool,
    pub playhead_beats: f64,
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub loop_enabled: bool,
    pub loop_end_beats: f64,
    pub drum_kit: Arc<DrumSampleKit>,
    pub sample_library: Arc<HashMap<String, Arc<AudioSample>>>,
    pub sample_preview: Option<SamplePreview>,
    pub sequencer_preview: Option<SequencerPreview>,
    pub metronome: PlaybackMetronome,
    pub drums: Vec<PlaybackDrumClip>,
    pub clips: Vec<PlaybackClip>,
}

#[derive(Clone)]
pub struct PlaybackMetronome {
    pub enabled_while_playing: bool,
    pub enabled_while_idle: bool,
    pub force_tick: bool,
    pub count_in_active: bool,
    pub count_in_start_beat: f64,
    pub sound: MetronomePedal,
}

#[derive(Clone)]
pub struct EngineConfig {
    pub route_enabled: bool,
    pub input_volume: f32,
    pub pedals: Vec<PedalSpec>,
    pub playback: PlaybackState,
    pub record_input: bool,
}

#[derive(Clone)]
struct Params {
    config: Arc<EngineConfig>,
    version: u64,
}

#[derive(Clone)]
pub struct RecordedTake {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

struct RecordingBuffer {
    samples: Vec<f32>,
    sample_rate: u32,
}

pub struct EngineHandle {
    stop_tx: mpsc::Sender<()>,
    join: thread::JoinHandle<Result<()>>,
    active: Arc<AtomicBool>,
    params: Arc<Mutex<Params>>,
    meters: Arc<Mutex<Meters>>,
    error: Arc<Mutex<Option<String>>>,
    latency: LatencyInfo,
    input_name: Option<String>,
    output_name: String,
    recording: Arc<Mutex<Option<RecordingBuffer>>>,
    sample_rate: Arc<AtomicU32>,
    transport_beats: Arc<AtomicU64>,
    preview_beats: Arc<AtomicU64>,
}

impl EngineHandle {
    pub fn start(input_name: Option<&str>, output_name: &str, config: EngineConfig) -> Result<Self> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        let params = Arc::new(Mutex::new(Params {
            config: Arc::new(config),
            version: 0,
        }));
        let meters = Arc::new(Mutex::new(Meters::default()));
        let error = Arc::new(Mutex::new(None));
        let recording = Arc::new(Mutex::new(None));
        let sample_rate = Arc::new(AtomicU32::new(44_100));
        let transport_beats = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let preview_beats = Arc::new(AtomicU64::new(0.0f64.to_bits()));

        let thread_active = Arc::clone(&active);
        let thread_params = Arc::clone(&params);
        let thread_meters = Arc::clone(&meters);
        let thread_error = Arc::clone(&error);
        let thread_recording = Arc::clone(&recording);
        let thread_sample_rate = Arc::clone(&sample_rate);
        let thread_transport = Arc::clone(&transport_beats);
        let thread_preview = Arc::clone(&preview_beats);
        let input_owned = input_name.map(str::to_string);
        let output_owned = output_name.to_string();
        let thread_input = input_owned.clone();
        let thread_output = output_owned.clone();

        let join = thread::spawn(move || {
            run_engine(
                thread_input.as_deref(),
                &thread_output,
                stop_rx,
                thread_active,
                ready_tx,
                thread_params,
                thread_meters,
                thread_error,
                thread_recording,
                thread_sample_rate,
                thread_transport,
                thread_preview,
            )
        });

        let latency = ready_rx
            .recv()
            .context("engine thread did not report readiness")??;

        Ok(Self {
            stop_tx,
            join,
            active,
            params,
            meters,
            error,
            latency,
            input_name: input_owned,
            output_name: output_owned,
            recording,
            sample_rate,
            transport_beats,
            preview_beats,
        })
    }

    pub fn update_config(&self, config: EngineConfig) -> Result<()> {
        let mut params = self.params.lock().map_err(|_| anyhow!("poisoned params mutex"))?;
        params.config = Arc::new(config);
        params.version = params.version.wrapping_add(1);
        Ok(())
    }

    pub fn begin_recording(&self) -> Result<()> {
        let mut recording = self.recording.lock().map_err(|_| anyhow!("poisoned recording mutex"))?;
        let sample_rate = self.sample_rate.load(Ordering::Acquire);
        *recording = Some(RecordingBuffer {
            samples: Vec::with_capacity(sample_rate as usize * 30),
            sample_rate,
        });
        Ok(())
    }

    pub fn take_recording(&self) -> Option<RecordedTake> {
        self.recording
            .lock()
            .ok()
            .and_then(|mut recording| recording.take())
            .map(|buffer| RecordedTake {
                samples: buffer.samples,
                sample_rate: buffer.sample_rate,
            })
    }

    pub fn current_playhead_beats(&self) -> f64 {
        f64::from_bits(self.transport_beats.load(Ordering::Acquire))
    }

    pub fn current_preview_beats(&self) -> f64 {
        f64::from_bits(self.preview_beats.load(Ordering::Acquire))
    }

    pub fn meters(&self) -> Meters {
        self.meters.lock().map(|meters| *meters).unwrap_or_default()
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut slot| slot.take())
    }

    pub fn stop(self) -> Result<()> {
        self.active.store(false, Ordering::Release);
        let _ = self.stop_tx.send(());
        self.join.join().map_err(|_| anyhow!("engine thread panicked"))?
    }

    pub fn latency_label(&self) -> String {
        self.latency.label()
    }

    pub fn input_name(&self) -> Option<&str> {
        self.input_name.as_deref()
    }

    pub fn output_name(&self) -> &str {
        self.output_name.as_str()
    }
}

fn run_engine(
    input_name: Option<&str>,
    output_name: &str,
    stop_rx: mpsc::Receiver<()>,
    active: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<Result<LatencyInfo>>,
    params: Arc<Mutex<Params>>,
    meters: Arc<Mutex<Meters>>,
    error: Arc<Mutex<Option<String>>>,
    recording: Arc<Mutex<Option<RecordingBuffer>>>,
    sample_rate: Arc<AtomicU32>,
    transport_beats: Arc<AtomicU64>,
    preview_beats: Arc<AtomicU64>,
) -> Result<()> {
    let host = cpal::default_host();
    let output = find_output_device(&host, output_name)?;
    let output_config = output.default_output_config()?;
    let output_stream_config = low_latency_stream_config(&output_config);
    let output_sample_rate = output_config.sample_rate().0;
    sample_rate.store(output_sample_rate, Ordering::Release);

    let input = input_name.map(|name| find_input_device(&host, name)).transpose()?;
    let input_pair = if let Some(input) = input {
        let input_config = input.default_input_config()?;
        let input_stream_config = low_latency_stream_config(&input_config);
        Some((input, input_config, input_stream_config))
    } else {
        None
    };

    let queue = Arc::new(AudioBuffer::new(LIVE_BUFFER_CAPACITY_FRAMES));
    let input_sample_rate = input_pair
        .as_ref()
        .map(|(_, config, _)| config.sample_rate().0)
        .unwrap_or(output_sample_rate);

    let input_stream = if let Some((input, input_config, input_stream_config)) = input_pair.as_ref() {
        Some(build_input_stream(
            input,
            input_config,
            input_stream_config,
            Arc::clone(&queue),
            Arc::clone(&active),
            Arc::clone(&error),
        )?)
    } else {
        None
    };

    let output_stream = build_output_stream(
        &output,
        &output_config,
        &output_stream_config,
        Arc::clone(&queue),
        input_sample_rate,
        Arc::clone(&active),
        Arc::clone(&params),
        Arc::clone(&meters),
        Arc::clone(&error),
        Arc::clone(&recording),
        Arc::clone(&transport_beats),
        Arc::clone(&preview_beats),
    )?;

    if let Some(stream) = input_stream.as_ref() {
        stream.play()?;
    }
    output_stream.play()?;
    let _ = ready_tx.send(Ok(LatencyInfo {
        sample_rate: output_sample_rate,
        input_buffer_frames: input_pair.as_ref().and_then(|(_, _, config)| stream_buffer_frames(&config.buffer_size)),
        output_buffer_frames: stream_buffer_frames(&output_stream_config.buffer_size),
        queue_target_frames: LIVE_BUFFER_TARGET_FRAMES,
    }));

    loop {
        if stop_rx.recv_timeout(Duration::from_millis(50)).is_ok() {
            break;
        }

        if error.lock().ok().and_then(|slot| slot.clone()).is_some() {
            break;
        }
    }

    active.store(false, Ordering::Release);
    Ok(())
}

fn find_input_device(host: &cpal::Host, wanted: &str) -> Result<cpal::Device> {
    for device in host.input_devices().context("failed to enumerate input devices")? {
        if device.name().ok().as_deref() == Some(wanted) {
            return Ok(device);
        }
    }
    Err(anyhow!("input device not found: {wanted}"))
}

fn find_output_device(host: &cpal::Host, wanted: &str) -> Result<cpal::Device> {
    for device in host.output_devices().context("failed to enumerate output devices")? {
        if device.name().ok().as_deref() == Some(wanted) {
            return Ok(device);
        }
    }
    Err(anyhow!("output device not found: {wanted}"))
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    stream_config: &cpal::StreamConfig,
    queue: Arc<AudioBuffer>,
    active: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream> {
    let channels = config.channels() as usize;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let queue_data = Arc::clone(&queue);
            let active_data = Arc::clone(&active);
            let error_data = Arc::clone(&error);
            let error_stream = Arc::clone(&error);
            device.build_input_stream(
                stream_config,
                move |data: &[f32], _| {
                    input_callback(
                        data.iter().copied(),
                        channels,
                        &queue_data,
                        &active_data,
                        &error_data,
                        |sample| sample.clamp(-1.0, 1.0),
                    )
                },
                move |err| store_error(err.to_string(), &error_stream),
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let queue_data = Arc::clone(&queue);
            let active_data = Arc::clone(&active);
            let error_data = Arc::clone(&error);
            let error_stream = Arc::clone(&error);
            device.build_input_stream(
                stream_config,
                move |data: &[i16], _| {
                    input_callback(
                        data.iter().copied(),
                        channels,
                        &queue_data,
                        &active_data,
                        &error_data,
                        |sample| sample as f32 / i16::MAX as f32,
                    )
                },
                move |err| store_error(err.to_string(), &error_stream),
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let queue_data = Arc::clone(&queue);
            let active_data = Arc::clone(&active);
            let error_data = Arc::clone(&error);
            let error_stream = Arc::clone(&error);
            device.build_input_stream(
                stream_config,
                move |data: &[u16], _| {
                    input_callback(
                        data.iter().copied(),
                        channels,
                        &queue_data,
                        &active_data,
                        &error_data,
                        |sample| (sample as f32 / 65535.0) * 2.0 - 1.0,
                    )
                },
                move |err| store_error(err.to_string(), &error_stream),
                None,
            )?
        }
        other => return Err(anyhow!("unsupported input sample format: {other:?}")),
    };

    Ok(stream)
}

fn input_callback<T, F>(
    data: impl Iterator<Item = T>,
    channels: usize,
    queue: &Arc<AudioBuffer>,
    active: &Arc<AtomicBool>,
    error: &Arc<Mutex<Option<String>>>,
    convert: F,
) where
    F: Fn(T) -> f32,
    T: Copy,
{
    if !active.load(Ordering::Acquire) || has_error(error) {
        return;
    }

    let channels = channels.max(1);
    let mut channel = 0;
    let mut mono = 0.0;
    for sample in data {
        mono += convert(sample);
        channel += 1;
        if channel == channels {
            let _ = queue.push(mono / channels as f32);
            channel = 0;
            mono = 0.0;
        }
    }
}

fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    stream_config: &cpal::StreamConfig,
    queue: Arc<AudioBuffer>,
    input_sample_rate: u32,
    active: Arc<AtomicBool>,
    params: Arc<Mutex<Params>>,
    meters: Arc<Mutex<Meters>>,
    error: Arc<Mutex<Option<String>>>,
    recording: Arc<Mutex<Option<RecordingBuffer>>>,
    transport_beats: Arc<AtomicU64>,
    preview_beats: Arc<AtomicU64>,
) -> Result<cpal::Stream> {
    let channels = config.channels() as usize;
    let sample_rate = config.sample_rate().0;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let queue_data = Arc::clone(&queue);
            let active_data = Arc::clone(&active);
            let params_data = Arc::clone(&params);
            let meters_data = Arc::clone(&meters);
            let error_data = Arc::clone(&error);
            let recording_data = Arc::clone(&recording);
            let error_stream = Arc::clone(&error);
            let transport_data = Arc::clone(&transport_beats);
            let preview_data = Arc::clone(&preview_beats);
            let mut runtime = PlaybackRuntime::new(sample_rate);
            let mut live = LiveInputRuntime::new(input_sample_rate, sample_rate);
            device.build_output_stream(
                stream_config,
                move |data: &mut [f32], _| {
                    output_callback(
                        data,
                        channels,
                        &queue_data,
                        &active_data,
                        &params_data,
                        &meters_data,
                        &error_data,
                        &recording_data,
                        &transport_data,
                        &preview_data,
                        &mut live,
                        &mut runtime,
                        |slot, sample| *slot = sample,
                    )
                },
                move |err| store_error(err.to_string(), &error_stream),
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let queue_data = Arc::clone(&queue);
            let active_data = Arc::clone(&active);
            let params_data = Arc::clone(&params);
            let meters_data = Arc::clone(&meters);
            let error_data = Arc::clone(&error);
            let recording_data = Arc::clone(&recording);
            let error_stream = Arc::clone(&error);
            let transport_data = Arc::clone(&transport_beats);
            let preview_data = Arc::clone(&preview_beats);
            let mut runtime = PlaybackRuntime::new(sample_rate);
            let mut live = LiveInputRuntime::new(input_sample_rate, sample_rate);
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], _| {
                    output_callback(
                        data,
                        channels,
                        &queue_data,
                        &active_data,
                        &params_data,
                        &meters_data,
                        &error_data,
                        &recording_data,
                        &transport_data,
                        &preview_data,
                        &mut live,
                        &mut runtime,
                        |slot, sample| *slot = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16,
                    )
                },
                move |err| store_error(err.to_string(), &error_stream),
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let queue_data = Arc::clone(&queue);
            let active_data = Arc::clone(&active);
            let params_data = Arc::clone(&params);
            let meters_data = Arc::clone(&meters);
            let error_data = Arc::clone(&error);
            let recording_data = Arc::clone(&recording);
            let error_stream = Arc::clone(&error);
            let transport_data = Arc::clone(&transport_beats);
            let preview_data = Arc::clone(&preview_beats);
            let mut runtime = PlaybackRuntime::new(sample_rate);
            let mut live = LiveInputRuntime::new(input_sample_rate, sample_rate);
            device.build_output_stream(
                stream_config,
                move |data: &mut [u16], _| {
                    output_callback(
                        data,
                        channels,
                        &queue_data,
                        &active_data,
                        &params_data,
                        &meters_data,
                        &error_data,
                        &recording_data,
                        &transport_data,
                        &preview_data,
                        &mut live,
                        &mut runtime,
                        |slot, sample| {
                            let scaled = ((sample.clamp(-1.0, 1.0) + 1.0) * 0.5 * 65535.0).round();
                            *slot = scaled.clamp(0.0, 65535.0) as u16;
                        },
                    )
                },
                move |err| store_error(err.to_string(), &error_stream),
                None,
            )?
        }
        other => return Err(anyhow!("unsupported output sample format: {other:?}")),
    };

    Ok(stream)
}

fn output_callback<T, F>(
    data: &mut [T],
    channels: usize,
    queue: &Arc<AudioBuffer>,
    active: &Arc<AtomicBool>,
    params: &Arc<Mutex<Params>>,
    meters: &Arc<Mutex<Meters>>,
    error: &Arc<Mutex<Option<String>>>,
    recording: &Arc<Mutex<Option<RecordingBuffer>>>,
    transport_beats: &Arc<AtomicU64>,
    preview_beats: &Arc<AtomicU64>,
    live: &mut LiveInputRuntime,
    runtime: &mut PlaybackRuntime,
    write_sample: F,
) where
    F: Fn(&mut T, f32),
{
    if !active.load(Ordering::Acquire) || has_error(error) {
        for sample in data.iter_mut() {
            write_sample(sample, 0.0);
        }
        return;
    }

    let (config, version) = match params.lock() {
        Ok(params) => (Arc::clone(&params.config), params.version),
        Err(_) => {
            store_error("poisoned params mutex".to_string(), error);
            return;
        }
    };

    runtime.sync(&config.playback, version);
    live.sync(&config.pedals);

    let mut input_peak = 0.0_f32;
    let mut output_peak = 0.0_f32;
    let mut recording = if config.record_input {
        recording.lock().ok()
    } else {
        None
    };
    for frame in data.chunks_mut(channels.max(1)) {
        let processed_input = live.next_sample(queue, &config.pedals) * config.input_volume;
        input_peak = input_peak.max(processed_input.abs());
        if let Some(buffer) = recording.as_deref_mut().and_then(Option::as_mut) {
            buffer.samples.push(processed_input);
        }

        let monitored_input = if config.route_enabled { processed_input } else { 0.0 };
        let playback = runtime.next_sample();
        let mixed = soft_limit(monitored_input + playback);
        output_peak = output_peak.max(mixed.abs());
        for slot in frame {
            write_sample(slot, mixed);
        }
    }

    transport_beats.store(runtime.playhead_beats.to_bits(), Ordering::Release);
    preview_beats.store(runtime.preview_playhead_beats().to_bits(), Ordering::Release);
    if let Ok(mut meters) = meters.try_lock() {
        meters.input_peak = input_peak.min(1.0);
        meters.output_peak = output_peak;
    }
}

struct LiveInputRuntime {
    resampler: InputResampler,
    chain: PedalChain,
}

impl LiveInputRuntime {
    fn new(input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            resampler: InputResampler::new(input_sample_rate, output_sample_rate),
            chain: PedalChain::new(output_sample_rate),
        }
    }

    fn sync(&mut self, pedals: &[PedalSpec]) {
        self.chain.sync(pedals);
    }

    fn next_sample(&mut self, queue: &AudioBuffer, pedals: &[PedalSpec]) -> f32 {
        self.chain.process(self.resampler.next_sample(queue), pedals)
    }
}

struct InputResampler {
    nominal_step: f64,
    step: f64,
    phase: f64,
    previous: f32,
    next: f32,
    last_output: f32,
    gain: f32,
    primed: bool,
    started: bool,
}

impl InputResampler {
    fn new(input_sample_rate: u32, output_sample_rate: u32) -> Self {
        let nominal_step = input_sample_rate.max(1) as f64 / output_sample_rate.max(1) as f64;
        Self {
            nominal_step,
            step: nominal_step,
            phase: 0.0,
            previous: 0.0,
            next: 0.0,
            last_output: 0.0,
            gain: 0.0,
            primed: false,
            started: false,
        }
    }

    fn next_sample(&mut self, queue: &AudioBuffer) -> f32 {
        if self.primed && queue.len() > LIVE_BUFFER_TARGET_FRAMES * 2 {
            while queue.len() > LIVE_BUFFER_TARGET_FRAMES {
                let _ = queue.pop();
            }
            self.primed = false;
            self.gain = 0.0;
        }

        if !self.primed {
            let prime_frames = if self.started { 64 } else { LIVE_BUFFER_TARGET_FRAMES };
            if queue.len() < prime_frames {
                self.gain *= 0.95;
                return self.last_output * self.gain;
            }

            let Some(previous) = queue.pop() else { return 0.0 };
            let Some(next) = queue.pop() else { return 0.0 };
            self.previous = previous;
            self.next = next;
            self.phase = 0.0;
            self.primed = true;
            self.started = true;
        }

        let occupancy_error = (queue.len() as f64 - LIVE_BUFFER_TARGET_FRAMES as f64)
            / LIVE_BUFFER_TARGET_FRAMES as f64;
        let correction = (occupancy_error * 0.002).clamp(-0.005, 0.005);
        let target_step = self.nominal_step * (1.0 + correction);
        self.step += (target_step - self.step) * 0.001;

        let output = self.previous + (self.next - self.previous) * self.phase as f32;
        self.phase += self.step;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.previous = self.next;
            let Some(next) = queue.pop() else {
                self.primed = false;
                break;
            };
            self.next = next;
        }

        self.last_output = output;
        self.gain = (self.gain + 1.0 / 64.0).min(1.0);
        output * self.gain
    }
}

fn soft_limit(sample: f32) -> f32 {
    const THRESHOLD: f32 = 0.9;
    let magnitude = sample.abs();
    if magnitude <= THRESHOLD {
        sample
    } else {
        sample.signum() * (THRESHOLD + (1.0 - THRESHOLD) * ((magnitude - THRESHOLD) / (1.0 - THRESHOLD)).tanh())
    }
}

struct PlaybackRuntime {
    sample_rate: f64,
    version: u64,
    playing: bool,
    playhead_beats: f64,
    bpm: f64,
    loop_enabled: bool,
    loop_end_beats: f64,
    drum_kit: Arc<DrumSampleKit>,
    sample_library: Arc<HashMap<String, Arc<AudioSample>>>,
    drums: Vec<DrumRuntimeClip>,
    clips: Vec<ClipRuntime>,
    voices: Vec<DrumVoice>,
    preview: Option<SequencerPreviewRuntime>,
    preview_audition_nonce: u64,
    preview_restart_nonce: u64,
    sample_preview_nonce: u64,
    metronome: MetronomeRuntime,
}

impl PlaybackRuntime {
    fn new(sample_rate: u32) -> Self {
        let silence = Arc::new(AudioSample {
            samples: Arc::from(vec![0.0_f32]),
            sample_rate_hz: sample_rate,
        });
        Self {
            sample_rate: sample_rate as f64,
            version: u64::MAX,
            playing: false,
            playhead_beats: 0.0,
            bpm: 110.0,
            loop_enabled: true,
            loop_end_beats: 16.0,
            drum_kit: Arc::new(DrumSampleKit {
                kick: vec![vec![Arc::clone(&silence)]],
                snare: vec![vec![Arc::clone(&silence)]],
                closed_hat: vec![vec![Arc::clone(&silence)]],
                open_hat: vec![vec![Arc::clone(&silence)]],
                pedal_hat: vec![vec![Arc::clone(&silence)]],
                high_tom: vec![vec![Arc::clone(&silence)]],
                low_tom: vec![vec![Arc::clone(&silence)]],
                ride: vec![vec![Arc::clone(&silence)]],
                crash: vec![vec![Arc::clone(&silence)]],
                percussion: vec![vec![silence]],
            }),
            sample_library: Arc::new(HashMap::new()),
            drums: Vec::new(),
            clips: Vec::new(),
            voices: Vec::with_capacity(128),
            preview: None,
            preview_audition_nonce: 0,
            preview_restart_nonce: 0,
            sample_preview_nonce: 0,
            metronome: MetronomeRuntime::new(sample_rate),
        }
    }

    fn sync(&mut self, playback: &PlaybackState, version: u64) {
        let preview_changed = self.version != version;
        if self.version != version {
            let had_preview = self.preview.is_some();
            let previous_preview_playing = self.preview.as_ref().is_some_and(|preview| preview.playing);
            let next_preview_exists = playback.sequencer_preview.is_some();
            let next_preview_playing = playback.sequencer_preview.as_ref().is_some_and(|preview| preview.playing);
            let next_restart_nonce = playback.sequencer_preview.as_ref().map_or(0, |preview| preview.restart_nonce);
            let preview_stopped_or_restarted = had_preview
                && (!next_preview_exists
                    || next_restart_nonce != self.preview_restart_nonce
                    || (previous_preview_playing && !next_preview_playing));
            if preview_stopped_or_restarted {
                release_sequencer_preview_voices(&mut self.voices);
            }
            let transport_jump = !self.playing
                || !playback.playing
                || (self.playhead_beats - playback.playhead_beats).abs() > 0.1;
            self.version = version;
            self.playing = playback.playing;
            if transport_jump {
                self.playhead_beats = playback.playhead_beats.max(0.0);
            }
            self.bpm = playback.bpm.max(1.0);
            self.loop_enabled = playback.loop_enabled;
            self.loop_end_beats = playback.loop_end_beats.max(1.0);
            self.drum_kit = Arc::clone(&playback.drum_kit);
            self.sample_library = Arc::clone(&playback.sample_library);
            self.metronome.sync(&playback.metronome, self.bpm, playback.beats_per_bar);
            self.drums = playback
                .drums
                .iter()
                .map(|clip| DrumRuntimeClip::new(clip.clone(), playback.beats_per_bar))
                .collect();
            self.clips = playback.clips.iter().cloned().map(ClipRuntime::new).collect();
            let previous_preview = self.preview.take();
            self.preview = playback.sequencer_preview.as_ref().map(|preview| {
                let mut next = SequencerPreviewRuntime::new(preview.clone(), playback.beats_per_bar);
                if playback
                    .sequencer_preview
                    .as_ref()
                    .is_some_and(|preview| preview.restart_nonce == self.preview_restart_nonce)
                {
                    if let Some(previous) = previous_preview.as_ref().filter(|previous| previous.playing && next.playing) {
                    next.playhead_beats = previous.playhead_beats.rem_euclid(next.length_beats().max(0.25));
                    next.completed_cycles = previous.completed_cycles;
                    }
                }
                next
            });
            self.preview_restart_nonce = playback.sequencer_preview.as_ref().map_or(0, |preview| preview.restart_nonce);
        } else {
            self.playing = playback.playing;
            self.bpm = playback.bpm.max(1.0);
            self.metronome.sync(&playback.metronome, self.bpm, playback.beats_per_bar);
        }

        if let Some(preview) = playback.sequencer_preview.as_ref() {
            if preview_changed {
                if preview.audition_nonce != self.preview_audition_nonce {
                    for lane in &preview.audition_lanes {
                        self.trigger_lane_voice(lane, &preview.sequence);
                    }
                }
                self.preview_audition_nonce = preview.audition_nonce;
            }
        } else {
            self.preview_audition_nonce = 0;
        }

        if let Some(preview) = playback.sample_preview.as_ref() {
            if preview.sample.is_none() {
                self.voices.retain(|voice| !voice.sample_preview);
                self.sample_preview_nonce = preview.nonce;
            } else if preview.nonce != self.sample_preview_nonce {
                self.sample_preview_nonce = preview.nonce;
                self.voices.retain(|voice| !voice.sample_preview);
                if let Some(sample) = preview.sample.as_ref() {
                    push_drum_voice(&mut self.voices, DrumVoice::new_sample_preview(Arc::clone(sample)));
                }
            }
        }
    }

    fn next_sample(&mut self) -> f32 {
        let mut mixed = 0.0_f32;
        if self.playing {
            let beat_step = self.bpm / 60.0 / self.sample_rate;
            let previous = self.playhead_beats;
            let next = previous + beat_step;
            self.metronome.advance_playing(previous, next, self.loop_enabled, self.loop_end_beats);
            let sample_library = Arc::clone(&self.sample_library);
            let fallback_kit = Arc::clone(&self.drum_kit);
            for drum in &mut self.drums {
                let sample_library = Arc::clone(&sample_library);
                let fallback_kit = Arc::clone(&fallback_kit);
                drum.collect_triggers(
                    previous,
                    next,
                    self.bpm,
                    self.loop_enabled,
                    self.loop_end_beats,
                    &move |lane_index, lane, variant, velocity| {
                        resolve_lane_sample(&sample_library, &fallback_kit, lane_index, lane, variant, velocity)
                    },
                    &mut self.voices,
                );
            }
            self.playhead_beats = if self.loop_enabled && next >= self.loop_end_beats {
                next - self.loop_end_beats
            } else {
                next
            };
        } else {
            self.metronome.advance_idle();
        }

        if let Some(preview) = &mut self.preview {
            if preview.playing {
                let beat_step = self.bpm / 60.0 / self.sample_rate;
                let previous = preview.playhead_beats;
                let next = previous + beat_step;
                let length = preview.length_beats().max(0.25);
                let sample_library = Arc::clone(&self.sample_library);
                let fallback_kit = Arc::clone(&self.drum_kit);
                preview.collect_triggers(
                    previous,
                    next,
                    self.bpm,
                    move |lane_index, lane, variant, velocity| {
                        resolve_lane_sample(&sample_library, &fallback_kit, lane_index, lane, variant, velocity)
                    },
                    &mut self.voices,
                );
                if next >= length {
                    preview.completed_cycles = preview.completed_cycles.wrapping_add(1);
                    preview.playhead_beats = next - length;
                } else {
                    preview.playhead_beats = next;
                }
            }
        }

        for clip in &mut self.clips {
            mixed += clip.sample_at(self.playhead_beats, self.playing, self.bpm, self.sample_rate);
        }

        let mut voice_index = 0;
        while voice_index < self.voices.len() {
            let sample = self.voices[voice_index].next_sample(self.sample_rate as f32);
            mixed += sample;
            if self.voices[voice_index].done {
                self.voices.swap_remove(voice_index);
            } else {
                voice_index += 1;
            }
        }

        mixed += self.metronome.next_sample();

        mixed
    }

    fn preview_playhead_beats(&self) -> f64 {
        self.preview.as_ref().map(|preview| preview.playhead_beats).unwrap_or(0.0)
    }

    fn trigger_lane_voice(&mut self, lane_index: &usize, sequence: &DrumSequence) {
        if let Some(lane) = sequence.lanes.get(*lane_index) {
            let samples = self.sample_for_lane(*lane_index, lane, 0, 0.82);
            push_drum_voice(
                &mut self.voices,
                DrumVoice::new_sequencer_preview(samples, lane.gain, lane.effective_role().choke_group()),
            );
        }
    }

    fn sample_for_lane(&self, lane_index: usize, lane: &DrumLane, variant: usize, velocity: f32) -> Arc<AudioSample> {
        resolve_lane_sample(&self.sample_library, &self.drum_kit, lane_index, lane, variant, velocity)
    }
}

struct SequencerPreviewRuntime {
    playing: bool,
    playhead_beats: f64,
    completed_cycles: u64,
    sequence: DrumSequence,
    beats_per_bar: u32,
}

impl SequencerPreviewRuntime {
    fn new(preview: SequencerPreview, beats_per_bar: u32) -> Self {
        Self {
            playing: preview.playing,
            playhead_beats: preview.playhead_beats.max(0.0),
            completed_cycles: 0,
            sequence: preview.sequence,
            beats_per_bar,
        }
    }

    fn length_beats(&self) -> f64 {
        (self.sequence.measures.max(1) * self.beats_per_bar.max(1)) as f64
    }

    fn collect_triggers(
        &self,
        previous_beat: f64,
        next_beat: f64,
        bpm: f64,
        sample_for_lane: impl Fn(usize, &DrumLane, usize, f32) -> Arc<AudioSample>,
        out: &mut Vec<DrumVoice>,
    ) {
        let length = self.length_beats();
        let absolute_start = self.completed_cycles as f64 * length + previous_beat;
        let absolute_end = self.completed_cycles as f64 * length + next_beat;
        collect_sequence_events(
            &self.sequence,
            self.beats_per_bar,
            absolute_start,
            absolute_end,
            bpm,
            1.0,
            absolute_start == 0.0,
            None,
            true,
            &sample_for_lane,
            out,
        );
    }
}

fn collect_sequence_events(
    sequence: &DrumSequence,
    beats_per_bar: u32,
    start_beat: f64,
    end_beat: f64,
    bpm: f64,
    volume: f32,
    include_start: bool,
    grid_end_exclusive: Option<f64>,
    sequencer_preview: bool,
    sample_for_lane: &impl Fn(usize, &DrumLane, usize, f32) -> Arc<AudioSample>,
    out: &mut Vec<DrumVoice>,
) {
    if end_beat < start_beat {
        return;
    }
    let total_steps = sequence.total_steps(beats_per_bar).max(1) as i64;
    let steps_per_beat = sequence.subdivision.steps_per_beat() as f64;
    let first_step = (start_beat * steps_per_beat).floor() as i64 - 2;
    let last_step = (end_beat * steps_per_beat).ceil() as i64 + 2;
    let epsilon = 1e-9;

    for absolute_step in first_step..=last_step {
        if absolute_step < 0 && start_beat >= 0.0 {
            continue;
        }
        let normalized = absolute_step.rem_euclid(total_steps) as usize;
        let raw_cycle = absolute_step.div_euclid(total_steps).max(0) as u64;
        for (lane_index, lane) in sequence.lanes.iter().enumerate() {
            if lane.muted || !lane.steps.get(normalized).copied().unwrap_or(false) {
                continue;
            }
            let settings = lane.setting(normalized);
            let cycle = if sequence.humanize.evolving { raw_cycle } else { 0 };
            let event_hash = drum_event_hash(sequence.humanize.seed, cycle, lane_index, normalized);
            if hash_unit(event_hash ^ 0xa511_e9b3) >= settings.probability as f64 {
                continue;
            }
            let role = lane.effective_role();
            let timing_scale = match role {
                DrumRole::Kick => 0.35,
                DrumRole::Snare => 0.7,
                DrumRole::ClosedHat | DrumRole::OpenHat | DrumRole::PedalHat => 1.0,
                _ => 0.8,
            };
            let random_timing_beats = hash_signed(event_hash ^ 0x72bd_43a7)
                * sequence.humanize.timing_ms as f64
                * timing_scale
                * bpm.max(1.0)
                / 60_000.0;
            let grid_beat = absolute_step as f64 / steps_per_beat;
            if grid_end_exclusive.is_some_and(|end| grid_beat >= end - epsilon) {
                continue;
            }
            let step_in_beat = absolute_step.rem_euclid(sequence.subdivision.steps_per_beat() as i64);
            let swung_offbeat = (sequence.subdivision.steps_per_beat() == 4 && matches!(step_in_beat, 1 | 3))
                || (sequence.subdivision.steps_per_beat() == 2 && step_in_beat == 1);
            let max_swing = if sequence.subdivision.steps_per_beat() == 4 { 1.0 / 12.0 } else { 1.0 / 6.0 };
            let swing_beats = if swung_offbeat { sequence.humanize.swing as f64 * max_swing } else { 0.0 };
            let feel_beats = sequence.humanize.feel_ms as f64 * bpm.max(1.0) / 60_000.0;
            let mut event_beat = grid_beat
                + settings.offset_steps as f64 / steps_per_beat
                + random_timing_beats
                + swing_beats
                + feel_beats;
            if include_start && grid_beat >= start_beat - epsilon && event_beat < start_beat {
                event_beat = start_beat;
            }
            let after_start = event_beat > start_beat + epsilon
                || (include_start && event_beat >= start_beat - epsilon);
            if !after_start || event_beat > end_beat + epsilon {
                continue;
            }
            let velocity = (settings.velocity
                + hash_signed(event_hash ^ 0x934d_21f0) as f32 * sequence.humanize.velocity_variation)
                .clamp(0.03, 1.0);
            let variant = mix_hash(event_hash ^ 0x5bd1_e995) as usize;
            let gain = volume * lane.gain * (0.3 + velocity * 0.7);
            let sample = sample_for_lane(lane_index, lane, variant, velocity);
            let voice = if sequencer_preview {
                DrumVoice::new_sequencer_preview(sample, gain, role.choke_group())
            } else {
                DrumVoice::new(sample, gain, role.choke_group())
            };
            push_drum_voice(out, voice);
        }
    }
}

fn drum_event_hash(seed: u64, cycle: u64, lane: usize, step: usize) -> u64 {
    let mut value = seed ^ cycle.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= (lane as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= (step as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb) ^ (value >> 31)
}

fn hash_unit(value: u64) -> f64 {
    (mix_hash(value) >> 11) as f64 / ((1_u64 << 53) - 1) as f64
}

fn mix_hash(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn hash_signed(value: u64) -> f64 {
    hash_unit(value) * 2.0 - 1.0
}

fn resolve_lane_sample(
    library: &HashMap<String, Arc<AudioSample>>,
    fallback: &DrumSampleKit,
    lane_index: usize,
    lane: &DrumLane,
    variant: usize,
    velocity: f32,
) -> Arc<AudioSample> {
    let path_count = lane.sample_path.is_some() as usize + lane.sample_variants.len();
    for attempt in 0..path_count {
        let selected = (variant + attempt) % path_count;
        let path = if lane.sample_path.is_some() && selected == 0 {
            lane.sample_path.as_ref()
        } else {
            lane.sample_variants.get(selected - lane.sample_path.is_some() as usize)
        };
        if let Some(sample) = path.and_then(|path| library.get(path)) {
            return Arc::clone(sample);
        }
    }
    fallback.sample_for_lane(lane_index, lane, variant, velocity)
}

fn push_drum_voice(voices: &mut Vec<DrumVoice>, voice: DrumVoice) {
    if let Some(group) = voice.choke_group {
        for active in voices
            .iter_mut()
            .filter(|active| active.choke_group == Some(group) && active.sequencer_preview == voice.sequencer_preview)
        {
            active.begin_choke();
        }
    }
    if voices.len() >= 128 {
        if let Some(index) = voices.iter().position(|voice| voice.done) {
            voices.swap_remove(index);
        } else {
            voices.swap_remove(0);
        }
    }
    voices.push(voice);
}

fn release_sequencer_preview_voices(voices: &mut [DrumVoice]) {
    for voice in voices.iter_mut().filter(|voice| voice.sequencer_preview) {
        voice.begin_choke();
    }
}

struct MetronomeRuntime {
    sample_rate: f32,
    bpm: f32,
    beats_per_bar: u32,
    enabled_while_playing: bool,
    enabled_while_idle: bool,
    force_tick: bool,
    count_in_active: bool,
    count_in_start_beat: f64,
    sound: MetronomePedal,
    idle_samples_until_beat: usize,
    idle_beat_index: u64,
    click_samples_left: usize,
    click_length: usize,
    phase: f32,
    click_freq: f32,
    click_amp: f32,
}

impl MetronomeRuntime {
    fn new(sample_rate: u32) -> Self {
        let click_length = ((sample_rate as f32) * 0.028) as usize;
        Self {
            sample_rate: sample_rate as f32,
            bpm: 110.0,
            beats_per_bar: 4,
            enabled_while_playing: true,
            enabled_while_idle: false,
            force_tick: false,
            count_in_active: false,
            count_in_start_beat: 0.0,
            sound: MetronomePedal::default(),
            idle_samples_until_beat: 0,
            idle_beat_index: 0,
            click_samples_left: 0,
            click_length: click_length.max(1),
            phase: 0.0,
            click_freq: 1200.0,
            click_amp: 0.0,
        }
    }

    fn sync(&mut self, metronome: &PlaybackMetronome, bpm: f64, beats_per_bar: u32) {
        let count_in_just_started = metronome.count_in_active
            && (!self.count_in_active || (self.count_in_start_beat - metronome.count_in_start_beat).abs() > f64::EPSILON);
        let idle_just_enabled = metronome.enabled_while_idle && !self.enabled_while_idle;

        self.enabled_while_playing = metronome.enabled_while_playing;
        self.enabled_while_idle = metronome.enabled_while_idle;
        self.force_tick = metronome.force_tick;
        self.count_in_active = metronome.count_in_active;
        self.count_in_start_beat = metronome.count_in_start_beat;
        self.sound = metronome.sound.clone();
        self.bpm = bpm.max(1.0) as f32;
        self.beats_per_bar = beats_per_bar.max(1);

        if count_in_just_started {
            self.idle_beat_index = metronome.count_in_start_beat.floor().max(0.0) as u64;
            self.idle_samples_until_beat = 0;
        } else if idle_just_enabled {
            self.idle_samples_until_beat = 0;
        }

        if !self.enabled_while_idle && !self.count_in_active {
            self.idle_samples_until_beat = 0;
        }
    }

    fn advance_playing(&mut self, previous_beat: f64, next_beat: f64, loop_enabled: bool, loop_end_beats: f64) {
        if !self.enabled_while_playing && !self.force_tick {
            return;
        }
        let previous_index = previous_beat.floor() as i64;
        let next_index = next_beat.floor() as i64;
        if loop_enabled && next_beat >= loop_end_beats {
            let wrapped = (next_beat - loop_end_beats).floor() as i64;
            self.trigger_click((wrapped.rem_euclid(self.beats_per_bar as i64)) == 0);
        } else if next_index != previous_index {
            self.trigger_click((next_index.rem_euclid(self.beats_per_bar as i64)) == 0);
        }
    }

    fn advance_idle(&mut self) {
        if !self.enabled_while_idle && !self.force_tick {
            return;
        }
        if self.idle_samples_until_beat == 0 {
            self.trigger_click(self.idle_beat_index % self.beats_per_bar as u64 == 0);
            self.idle_beat_index = self.idle_beat_index.wrapping_add(1);
            self.idle_samples_until_beat = self.samples_per_beat();
        }
        self.idle_samples_until_beat = self.idle_samples_until_beat.saturating_sub(1);
    }

    fn next_sample(&mut self) -> f32 {
        if self.click_samples_left == 0 {
            return 0.0;
        }
        let progress = 1.0 - self.click_samples_left as f32 / self.click_length as f32;
        let envelope = (1.0 - progress).powf(2.6);
        self.phase = (self.phase + std::f32::consts::TAU * self.click_freq / self.sample_rate) % std::f32::consts::TAU;
        self.click_samples_left = self.click_samples_left.saturating_sub(1);
        (self.phase.sin() * envelope * self.click_amp).clamp(-1.0, 1.0)
    }

    fn trigger_click(&mut self, accented: bool) {
        self.click_samples_left = self.click_length;
        self.phase = 0.0;
        self.click_freq = if accented {
            self.sound.tone_hz * 1.55
        } else {
            self.sound.tone_hz.max(120.0)
        };
        self.click_amp = if accented { 1.0 } else { 0.58 } * self.sound.volume.max(0.0);
    }

    fn samples_per_beat(&self) -> usize {
        ((60.0 / self.bpm.max(1.0)) * self.sample_rate).round().max(1.0) as usize
    }
}

struct DrumRuntimeClip {
    active: bool,
    start_beat: f64,
    length_beats: f64,
    source_offset_beats: f64,
    loop_count: f64,
    volume: f32,
    sequence: DrumSequence,
    beats_per_bar: u32,
}

impl DrumRuntimeClip {
    fn new(clip: PlaybackDrumClip, beats_per_bar: u32) -> Self {
        Self {
            active: clip.active,
            start_beat: clip.start_beat,
            length_beats: clip.length_beats,
            source_offset_beats: clip.source_offset_beats,
            loop_count: clip.loop_count,
            volume: clip.volume,
            sequence: clip.sequence,
            beats_per_bar,
        }
    }

    fn collect_triggers(
        &mut self,
        previous_beat: f64,
        next_beat: f64,
        bpm: f64,
        loop_enabled: bool,
        loop_end_beats: f64,
        sample_for_lane: &impl Fn(usize, &DrumLane, usize, f32) -> Arc<AudioSample>,
        out: &mut Vec<DrumVoice>,
    ) {
        if !self.active {
            return;
        }
        if loop_enabled && next_beat >= loop_end_beats {
            self.trigger_segment(previous_beat, loop_end_beats, bpm, true, sample_for_lane, out);
            self.trigger_segment(0.0, next_beat - loop_end_beats, bpm, false, sample_for_lane, out);
        } else {
            self.trigger_segment(previous_beat, next_beat, bpm, false, sample_for_lane, out);
        }
    }

    fn trigger_segment(
        &self,
        previous_beat: f64,
        next_beat: f64,
        bpm: f64,
        end_exclusive: bool,
        sample_for_lane: &impl Fn(usize, &DrumLane, usize, f32) -> Arc<AudioSample>,
        out: &mut Vec<DrumVoice>,
    ) {
        let base_length = self.length_beats.max(0.25);
        let clip_end = self.start_beat + base_length * self.loop_count.max(0.25);
        if next_beat <= self.start_beat || previous_beat >= clip_end {
            return;
        }

        let clipped_prev = previous_beat.max(self.start_beat);
        let clipped_next = next_beat.min(clip_end);
        if clipped_next <= clipped_prev {
            return;
        }

        let sequence_start = self.source_offset_beats + (clipped_prev - self.start_beat);
        let sequence_end = self.source_offset_beats + (clipped_next - self.start_beat);
        collect_sequence_events(
            &self.sequence,
            self.beats_per_bar,
            sequence_start,
            sequence_end,
            bpm,
            self.volume,
            clipped_prev == self.start_beat,
            Some(if end_exclusive {
                sequence_end.min(self.source_offset_beats + (clip_end - self.start_beat))
            } else {
                self.source_offset_beats + (clip_end - self.start_beat)
            }),
            false,
            sample_for_lane,
            out,
        );
    }
}

#[derive(Clone)]
struct ClipRuntime {
    start_beat: f64,
    length_beats: f64,
    source_offset_beats: f64,
    loop_count: f64,
    sample_rate_hz: u32,
    volume: f32,
    samples: Arc<[f32]>,
}

impl ClipRuntime {
    fn new(clip: PlaybackClip) -> Self {
        Self {
            start_beat: clip.start_beat,
            length_beats: clip.length_beats,
            source_offset_beats: clip.source_offset_beats,
            loop_count: clip.loop_count,
            sample_rate_hz: clip.sample_rate_hz,
            volume: clip.volume,
            samples: clip.samples,
        }
    }

    fn sample_at(&mut self, playhead_beats: f64, playing: bool, bpm: f64, _sample_rate: f64) -> f32 {
        let span_beats = (self.length_beats.max(0.25) * self.loop_count.max(0.25)).max(0.25);
        if !playing || playhead_beats < self.start_beat || playhead_beats >= self.start_beat + span_beats {
            return 0.0;
        }
        let relative = (playhead_beats - self.start_beat).rem_euclid(self.length_beats.max(0.25));
        let seconds = (self.source_offset_beats + relative) * 60.0 / bpm.max(1.0);
        let position = seconds * self.sample_rate_hz.max(1) as f64;
        let sample_index = position.floor() as usize;
        let Some(current) = self.samples.get(sample_index).copied() else {
            return 0.0;
        };
        let next = self.samples.get(sample_index + 1).copied().unwrap_or(current);
        let fraction = (position - sample_index as f64) as f32;
        (current + (next - current) * fraction) * self.volume
    }
}

struct DrumVoice {
    sample: Arc<AudioSample>,
    position: f64,
    volume: f32,
    choke_group: Option<u8>,
    sample_preview: bool,
    sequencer_preview: bool,
    release_frames: Option<u32>,
    done: bool,
}

impl DrumVoice {
    fn new(sample: Arc<AudioSample>, volume: f32, choke_group: Option<u8>) -> Self {
        Self {
            sample,
            position: 0.0,
            volume,
            choke_group,
            sample_preview: false,
            sequencer_preview: false,
            release_frames: None,
            done: false,
        }
    }

    fn new_sample_preview(sample: Arc<AudioSample>) -> Self {
        let mut voice = Self::new(sample, 1.0, None);
        voice.sample_preview = true;
        voice
    }

    fn new_sequencer_preview(sample: Arc<AudioSample>, volume: f32, choke_group: Option<u8>) -> Self {
        let mut voice = Self::new(sample, volume, choke_group);
        voice.sequencer_preview = true;
        voice
    }

    fn begin_choke(&mut self) {
        self.release_frames = Some(128);
    }

    fn next_sample(&mut self, output_sample_rate: f32) -> f32 {
        let index = self.position.floor() as usize;
        let Some(current) = self.sample.samples.get(index).copied() else {
            self.done = true;
            return 0.0;
        };
        let next = self.sample.samples.get(index + 1).copied().unwrap_or(current);
        let fraction = (self.position - index as f64) as f32;
        self.position += self.sample.sample_rate_hz.max(1) as f64 / output_sample_rate.max(1.0) as f64;
        let release_gain = if let Some(frames) = self.release_frames.as_mut() {
            let gain = *frames as f32 / 128.0;
            *frames = frames.saturating_sub(1);
            if *frames == 0 {
                self.done = true;
            }
            gain
        } else {
            1.0
        };
        (current + (next - current) * fraction) * self.volume * release_gain
    }
}

fn has_error(error: &Arc<Mutex<Option<String>>>) -> bool {
    error.lock().map(|slot| slot.is_some()).unwrap_or(true)
}

fn store_error(message: String, error: &Arc<Mutex<Option<String>>>) {
    if let Ok(mut slot) = error.lock() {
        if slot.is_none() {
            *slot = Some(message);
        }
    }
}

struct AudioBuffer {
    samples: Box<[AtomicU32]>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl AudioBuffer {
    fn new(capacity: usize) -> Self {
        let samples = (0..capacity)
            .map(|_| AtomicU32::new(0.0f32.to_bits()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            samples,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn push(&self, sample: f32) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.capacity {
            return false;
        }

        let slot = head % self.capacity;
        self.samples[slot].store(sample.to_bits(), Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    fn pop(&self) -> Option<f32> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        if head == tail {
            return None;
        }

        let slot = tail % self.capacity;
        let sample = f32::from_bits(self.samples[slot].load(Ordering::Relaxed));
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(sample)
    }

    fn len(&self) -> usize {
        self.head
            .load(Ordering::Acquire)
            .wrapping_sub(self.tail.load(Ordering::Acquire))
            .min(self.capacity)
    }
}

#[derive(Clone, Copy)]
struct LatencyInfo {
    sample_rate: u32,
    input_buffer_frames: Option<u32>,
    output_buffer_frames: Option<u32>,
    queue_target_frames: usize,
}

impl LatencyInfo {
    fn label(&self) -> String {
        let total_frames = self.input_buffer_frames.unwrap_or(0) as usize
            + self.output_buffer_frames.unwrap_or(0) as usize
            + self.queue_target_frames;
        let total_ms = total_frames as f32 * 1000.0 / self.sample_rate as f32;

        format!(
            "~{total_ms:.1} ms, {} Hz, in {}, out {}, queue {}",
            self.sample_rate,
            buffer_label(self.input_buffer_frames),
            buffer_label(self.output_buffer_frames),
            self.queue_target_frames
        )
    }
}

fn low_latency_stream_config(config: &cpal::SupportedStreamConfig) -> cpal::StreamConfig {
    let mut stream_config = config.config();
    stream_config.buffer_size = preferred_buffer_size(config.buffer_size());
    stream_config
}

fn preferred_buffer_size(size: &cpal::SupportedBufferSize) -> cpal::BufferSize {
    match *size {
        cpal::SupportedBufferSize::Range { min, max } => {
            let preferred = STREAM_BUFFER_FRAMES as u32;
            cpal::BufferSize::Fixed(preferred.clamp(min, max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    }
}

fn stream_buffer_frames(size: &cpal::BufferSize) -> Option<u32> {
    match *size {
        cpal::BufferSize::Fixed(frames) => Some(frames),
        cpal::BufferSize::Default => None,
    }
}

fn buffer_label(size: Option<u32>) -> String {
    match size {
        Some(frames) => frames.to_string(),
        None => "default".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_buffer_preserves_order_and_rejects_overflow() {
        let buffer = AudioBuffer::new(3);
        assert!(buffer.push(1.0));
        assert!(buffer.push(2.0));
        assert!(buffer.push(3.0));
        assert!(!buffer.push(4.0));
        assert_eq!(buffer.pop(), Some(1.0));
        assert_eq!(buffer.pop(), Some(2.0));
        assert_eq!(buffer.pop(), Some(3.0));
        assert_eq!(buffer.pop(), None);
    }

    #[test]
    fn resampler_preserves_pitch_across_device_sample_rates() {
        const INPUT_RATE: u32 = 44_100;
        const OUTPUT_RATE: u32 = 48_000;
        const FREQUENCY: f64 = 440.0;

        let buffer = AudioBuffer::new(LIVE_BUFFER_CAPACITY_FRAMES);
        let mut source_index = 0_u64;
        let push_source = |buffer: &AudioBuffer, source_index: &mut u64| {
            let phase = std::f64::consts::TAU * FREQUENCY * *source_index as f64 / INPUT_RATE as f64;
            assert!(buffer.push(phase.sin() as f32));
            *source_index += 1;
        };
        for _ in 0..LIVE_BUFFER_TARGET_FRAMES {
            push_source(&buffer, &mut source_index);
        }

        let mut resampler = InputResampler::new(INPUT_RATE, OUTPUT_RATE);
        let mut producer_phase = 0.0_f64;
        let mut output = Vec::with_capacity(OUTPUT_RATE as usize * 2);
        for _ in 0..OUTPUT_RATE * 2 {
            producer_phase += INPUT_RATE as f64 / OUTPUT_RATE as f64;
            while producer_phase >= 1.0 {
                push_source(&buffer, &mut source_index);
                producer_phase -= 1.0;
            }
            output.push(resampler.next_sample(&buffer));
        }

        let steady = &output[OUTPUT_RATE as usize..];
        let crossings = steady
            .windows(2)
            .filter(|samples| samples[0] <= 0.0 && samples[1] > 0.0)
            .count();
        assert!((crossings as i32 - FREQUENCY as i32).abs() <= 2, "measured {crossings} Hz");
    }

    #[test]
    fn drum_voice_uses_the_sample_source_rate() {
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 240]),
            sample_rate_hz: 24_000,
        });
        let mut voice = DrumVoice::new(sample, 1.0, None);
        let mut rendered_frames = 0;
        while !voice.done {
            let _ = voice.next_sample(48_000.0);
            rendered_frames += 1;
        }

        assert_eq!(rendered_frames, 481);
    }

    #[test]
    fn resampler_discards_stale_backlog() {
        let buffer = AudioBuffer::new(LIVE_BUFFER_CAPACITY_FRAMES);
        for _ in 0..LIVE_BUFFER_TARGET_FRAMES {
            assert!(buffer.push(0.0));
        }
        let mut resampler = InputResampler::new(48_000, 48_000);
        let _ = resampler.next_sample(&buffer);
        while buffer.len() <= LIVE_BUFFER_TARGET_FRAMES * 2 {
            assert!(buffer.push(0.0));
        }

        let _ = resampler.next_sample(&buffer);
        assert!(buffer.len() < LIVE_BUFFER_TARGET_FRAMES);
    }

    #[test]
    fn expressive_scheduler_triggers_step_zero_with_velocity() {
        let mut sequence = DrumSequence::default();
        sequence.measures = 1;
        sequence.humanize.timing_ms = 0.0;
        sequence.humanize.velocity_variation = 0.0;
        let mut lane = DrumLane::new("Kick", None);
        lane.steps = vec![true; 16];
        lane.step_settings = vec![crate::project::DrumStepSettings::default(); 16];
        lane.step_settings[0].velocity = 0.5;
        lane.steps.iter_mut().skip(1).for_each(|step| *step = false);
        sequence.lanes.push(lane);
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 8]),
            sample_rate_hz: 48_000,
        });
        let mut voices = Vec::new();
        collect_sequence_events(
            &sequence,
            4,
            0.0,
            0.01,
            120.0,
            1.0,
            true,
            None,
            false,
            &|_, _, _, _| Arc::clone(&sample),
            &mut voices,
        );
        assert_eq!(voices.len(), 1);
        assert!((voices[0].volume - 0.65).abs() < 0.001);
    }

    #[test]
    fn expressive_scheduler_honors_probability() {
        let mut sequence = DrumSequence::default();
        sequence.measures = 1;
        sequence.humanize.timing_ms = 0.0;
        let mut lane = DrumLane::new("Snare", None);
        lane.steps = vec![false; 16];
        lane.steps[0] = true;
        lane.step_settings = vec![crate::project::DrumStepSettings::default(); 16];
        lane.step_settings[0].probability = 0.0;
        sequence.lanes.push(lane);
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 8]),
            sample_rate_hz: 48_000,
        });
        let mut voices = Vec::new();
        collect_sequence_events(
            &sequence,
            4,
            0.0,
            0.01,
            120.0,
            1.0,
            true,
            None,
            false,
            &|_, _, _, _| Arc::clone(&sample),
            &mut voices,
        );
        assert!(voices.is_empty());
    }

    #[test]
    fn expressive_scheduler_does_not_retrigger_step_zero_at_clip_end() {
        let mut sequence = DrumSequence::default();
        sequence.measures = 1;
        sequence.humanize = crate::project::DrumHumanize {
            timing_ms: 0.0,
            velocity_variation: 0.0,
            swing: 0.0,
            feel_ms: 0.0,
            seed: 1,
            evolving: false,
        };
        let mut lane = DrumLane::new("Kick", None);
        lane.steps = vec![false; 16];
        lane.steps[0] = true;
        lane.step_settings = vec![crate::project::DrumStepSettings::default(); 16];
        sequence.lanes.push(lane);
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 8]),
            sample_rate_hz: 48_000,
        });
        let mut voices = Vec::new();
        collect_sequence_events(
            &sequence,
            4,
            3.99,
            4.0,
            120.0,
            1.0,
            false,
            Some(4.0),
            false,
            &|_, _, _, _| Arc::clone(&sample),
            &mut voices,
        );
        assert!(voices.is_empty());
    }

    #[test]
    fn closed_hat_chokes_an_open_hat_voice() {
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 8]),
            sample_rate_hz: 48_000,
        });
        let mut voices = Vec::new();
        push_drum_voice(&mut voices, DrumVoice::new(Arc::clone(&sample), 1.0, DrumRole::OpenHat.choke_group()));
        push_drum_voice(&mut voices, DrumVoice::new(sample, 1.0, DrumRole::ClosedHat.choke_group()));
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].release_frames, Some(128));
        assert!(!voices[1].done);
    }

    #[test]
    fn stopping_preview_releases_only_preview_voices() {
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 8]),
            sample_rate_hz: 48_000,
        });
        let mut voices = vec![
            DrumVoice::new_sequencer_preview(Arc::clone(&sample), 1.0, None),
            DrumVoice::new(sample, 1.0, None),
        ];

        release_sequencer_preview_voices(&mut voices);

        assert_eq!(voices[0].release_frames, Some(128));
        assert_eq!(voices[1].release_frames, None);
    }

    #[test]
    fn preview_hat_does_not_choke_arranged_hat() {
        let sample = Arc::new(AudioSample {
            samples: Arc::from(vec![1.0_f32; 8]),
            sample_rate_hz: 48_000,
        });
        let mut voices = vec![DrumVoice::new(
            Arc::clone(&sample),
            1.0,
            DrumRole::OpenHat.choke_group(),
        )];

        push_drum_voice(
            &mut voices,
            DrumVoice::new_sequencer_preview(sample, 1.0, DrumRole::ClosedHat.choke_group()),
        );

        assert_eq!(voices[0].release_frames, None);
        assert_eq!(voices[1].release_frames, None);
    }
}
