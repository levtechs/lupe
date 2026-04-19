use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::pedals::{MetronomePedal, PedalChain, PedalSpec};
use crate::project::{DrumLane, DrumSequence};

const TARGET_BUFFER_FRAMES: usize = 96;
const MAX_BUFFER_FRAMES: usize = 384;

#[derive(Default, Clone, Copy)]
pub struct Meters {
    pub input_peak: f32,
    pub output_peak: f32,
}

#[derive(Clone)]
pub struct PlaybackDrumTrack {
    pub active: bool,
    pub volume: f32,
    pub sequence: DrumSequence,
}

#[derive(Clone)]
pub struct PlaybackClip {
    pub start_beat: f64,
    pub length_beats: f64,
    pub source_offset_beats: f64,
    pub loop_count: f64,
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
    pub metronome: PlaybackMetronome,
    pub drums: Vec<PlaybackDrumTrack>,
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
    config: EngineConfig,
    version: u64,
}

#[derive(Clone)]
pub struct RecordedTake {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

struct RecordingBuffer {
    samples: Vec<f32>,
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
}

impl EngineHandle {
    pub fn start(input_name: Option<&str>, output_name: &str, config: EngineConfig) -> Result<Self> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        let params = Arc::new(Mutex::new(Params { config, version: 0 }));
        let meters = Arc::new(Mutex::new(Meters::default()));
        let error = Arc::new(Mutex::new(None));
        let recording = Arc::new(Mutex::new(None));
        let sample_rate = Arc::new(AtomicU32::new(44_100));
        let transport_beats = Arc::new(AtomicU64::new(0.0f64.to_bits()));

        let thread_active = Arc::clone(&active);
        let thread_params = Arc::clone(&params);
        let thread_meters = Arc::clone(&meters);
        let thread_error = Arc::clone(&error);
        let thread_recording = Arc::clone(&recording);
        let thread_sample_rate = Arc::clone(&sample_rate);
        let thread_transport = Arc::clone(&transport_beats);
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
        })
    }

    pub fn update_config(&self, config: EngineConfig) -> Result<()> {
        let mut params = self.params.lock().map_err(|_| anyhow!("poisoned params mutex"))?;
        params.config = config;
        params.version = params.version.wrapping_add(1);
        Ok(())
    }

    pub fn begin_recording(&self) -> Result<()> {
        let mut recording = self.recording.lock().map_err(|_| anyhow!("poisoned recording mutex"))?;
        *recording = Some(RecordingBuffer { samples: Vec::new() });
        Ok(())
    }

    pub fn take_recording(&self) -> Option<RecordedTake> {
        let sample_rate = self.sample_rate.load(Ordering::Acquire);
        self.recording
            .lock()
            .ok()
            .and_then(|mut recording| recording.take())
            .map(|buffer| RecordedTake {
                samples: buffer.samples,
                sample_rate,
            })
    }

    pub fn current_playhead_beats(&self) -> f64 {
        f64::from_bits(self.transport_beats.load(Ordering::Acquire))
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

    let queue = Arc::new(AudioBuffer::new(TARGET_BUFFER_FRAMES, MAX_BUFFER_FRAMES));

    let input_stream = if let Some((input, input_config, input_stream_config)) = input_pair.as_ref() {
        Some(build_input_stream(
            input,
            input_config,
            input_stream_config,
            Arc::clone(&queue),
            Arc::clone(&active),
            Arc::clone(&params),
            Arc::clone(&meters),
            Arc::clone(&error),
            Arc::clone(&recording),
        )?)
    } else {
        None
    };

    let output_stream = build_output_stream(
        &output,
        &output_config,
        &output_stream_config,
        Arc::clone(&queue),
        Arc::clone(&active),
        Arc::clone(&params),
        Arc::clone(&meters),
        Arc::clone(&error),
        Arc::clone(&transport_beats),
    )?;

    if let Some(stream) = input_stream.as_ref() {
        stream.play()?;
    }
    output_stream.play()?;
    let _ = ready_tx.send(Ok(LatencyInfo {
        sample_rate: output_sample_rate,
        input_buffer_frames: input_pair.as_ref().and_then(|(_, _, config)| stream_buffer_frames(&config.buffer_size)),
        output_buffer_frames: stream_buffer_frames(&output_stream_config.buffer_size),
        queue_target_frames: TARGET_BUFFER_FRAMES,
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
    params: Arc<Mutex<Params>>,
    meters: Arc<Mutex<Meters>>,
    error: Arc<Mutex<Option<String>>>,
    recording: Arc<Mutex<Option<RecordingBuffer>>>,
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
            let error_stream = Arc::clone(&error);
            let recording_data = Arc::clone(&recording);
            let mut chain = PedalChain::new(sample_rate);
            device.build_input_stream(
                stream_config,
                move |data: &[f32], _| {
                    input_callback(
                        data.iter().copied(),
                        channels,
                        &queue_data,
                        &active_data,
                        &params_data,
                        &meters_data,
                        &error_data,
                        &recording_data,
                        &mut chain,
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
            let params_data = Arc::clone(&params);
            let meters_data = Arc::clone(&meters);
            let error_data = Arc::clone(&error);
            let error_stream = Arc::clone(&error);
            let recording_data = Arc::clone(&recording);
            let mut chain = PedalChain::new(sample_rate);
            device.build_input_stream(
                stream_config,
                move |data: &[i16], _| {
                    input_callback(
                        data.iter().copied(),
                        channels,
                        &queue_data,
                        &active_data,
                        &params_data,
                        &meters_data,
                        &error_data,
                        &recording_data,
                        &mut chain,
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
            let params_data = Arc::clone(&params);
            let meters_data = Arc::clone(&meters);
            let error_data = Arc::clone(&error);
            let error_stream = Arc::clone(&error);
            let recording_data = Arc::clone(&recording);
            let mut chain = PedalChain::new(sample_rate);
            device.build_input_stream(
                stream_config,
                move |data: &[u16], _| {
                    input_callback(
                        data.iter().copied(),
                        channels,
                        &queue_data,
                        &active_data,
                        &params_data,
                        &meters_data,
                        &error_data,
                        &recording_data,
                        &mut chain,
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
    params: &Arc<Mutex<Params>>,
    meters: &Arc<Mutex<Meters>>,
    error: &Arc<Mutex<Option<String>>>,
    recording: &Arc<Mutex<Option<RecordingBuffer>>>,
    chain: &mut PedalChain,
    convert: F,
) where
    F: Fn(T) -> f32,
    T: Copy,
{
    if !active.load(Ordering::Acquire) || has_error(error) {
        return;
    }

    let params = match params.lock() {
        Ok(params) => params.clone(),
        Err(_) => {
            store_error("poisoned params mutex".to_string(), error);
            return;
        }
    };
    chain.sync(&params.config.pedals, params.version);

    let mut peak = 0.0_f32;
    let mut frame = Vec::with_capacity(channels.max(1));
    let mut recorded_chunk = Vec::new();
    for sample in data {
        frame.push(convert(sample));
        if frame.len() < channels.max(1) {
            continue;
        }

        let processed = (chain.process(frame[0], &params.config.pedals) * params.config.input_volume).clamp(-1.0, 1.0);
        peak = peak.max(processed.abs());
        queue.push(processed);
        if params.config.record_input {
            recorded_chunk.push(processed);
        }
        frame.clear();
    }

    if !frame.is_empty() {
        let processed = (chain.process(frame[0], &params.config.pedals) * params.config.input_volume).clamp(-1.0, 1.0);
        peak = peak.max(processed.abs());
        queue.push(processed);
        if params.config.record_input {
            recorded_chunk.push(processed);
        }
    }

    if params.config.record_input && !recorded_chunk.is_empty() {
        if let Ok(mut recording) = recording.lock() {
            if let Some(buffer) = recording.as_mut() {
                buffer.samples.extend(recorded_chunk);
            }
        }
    }

    if let Ok(mut meters) = meters.lock() {
        meters.input_peak = peak;
    }
}

fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    stream_config: &cpal::StreamConfig,
    queue: Arc<AudioBuffer>,
    active: Arc<AtomicBool>,
    params: Arc<Mutex<Params>>,
    meters: Arc<Mutex<Meters>>,
    error: Arc<Mutex<Option<String>>>,
    transport_beats: Arc<AtomicU64>,
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
            let error_stream = Arc::clone(&error);
            let transport_data = Arc::clone(&transport_beats);
            let mut runtime = PlaybackRuntime::new(sample_rate);
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
                        &transport_data,
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
            let error_stream = Arc::clone(&error);
            let transport_data = Arc::clone(&transport_beats);
            let mut runtime = PlaybackRuntime::new(sample_rate);
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
                        &transport_data,
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
            let error_stream = Arc::clone(&error);
            let transport_data = Arc::clone(&transport_beats);
            let mut runtime = PlaybackRuntime::new(sample_rate);
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
                        &transport_data,
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
    transport_beats: &Arc<AtomicU64>,
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

    let params = match params.lock() {
        Ok(params) => params.clone(),
        Err(_) => {
            store_error("poisoned params mutex".to_string(), error);
            return;
        }
    };

    runtime.sync(&params.config.playback, params.version);

    let mut peak = 0.0_f32;
    for frame in data.chunks_mut(channels.max(1)) {
        let monitored_input = queue.pop();
        let live = if params.config.route_enabled { monitored_input } else { 0.0 };
        let playback = runtime.next_sample();
        let mixed = (live + playback).clamp(-1.0, 1.0);
        peak = peak.max(mixed.abs());
        for slot in frame {
            write_sample(slot, mixed);
        }
    }

    transport_beats.store(runtime.playhead_beats.to_bits(), Ordering::Release);
    if let Ok(mut meters) = meters.lock() {
        meters.output_peak = peak;
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
    drums: Vec<DrumRuntimeTrack>,
    clips: Vec<ClipRuntime>,
    voices: Vec<DrumVoice>,
    metronome: MetronomeRuntime,
}

impl PlaybackRuntime {
    fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate as f64,
            version: u64::MAX,
            playing: false,
            playhead_beats: 0.0,
            bpm: 110.0,
            loop_enabled: true,
            loop_end_beats: 16.0,
            drums: Vec::new(),
            clips: Vec::new(),
            voices: Vec::new(),
            metronome: MetronomeRuntime::new(sample_rate),
        }
    }

    fn sync(&mut self, playback: &PlaybackState, version: u64) {
        if self.version != version {
            self.version = version;
            self.playing = playback.playing;
            self.playhead_beats = playback.playhead_beats.max(0.0);
            self.bpm = playback.bpm.max(1.0);
            self.loop_enabled = playback.loop_enabled;
            self.loop_end_beats = playback.loop_end_beats.max(1.0);
            self.metronome.sync(&playback.metronome, self.bpm, playback.beats_per_bar);
            self.drums = playback
                .drums
                .iter()
                .map(|track| DrumRuntimeTrack::new(track.active, track.volume, track.sequence.clone(), playback.beats_per_bar))
                .collect();
            self.clips = playback.clips.iter().cloned().map(ClipRuntime::new).collect();
            self.voices.clear();
        } else {
            self.playing = playback.playing;
            self.bpm = playback.bpm.max(1.0);
            self.metronome.sync(&playback.metronome, self.bpm, playback.beats_per_bar);
        }
    }

    fn next_sample(&mut self) -> f32 {
        let mut mixed = 0.0_f32;
        if self.playing {
            let beat_step = self.bpm / 60.0 / self.sample_rate;
            let previous = self.playhead_beats;
            let next = previous + beat_step;
            self.metronome.advance_playing(previous, next, self.loop_enabled, self.loop_end_beats);
            for drum in &mut self.drums {
                drum.collect_triggers(previous, next, self.loop_enabled, self.loop_end_beats, &mut self.voices);
            }
            self.playhead_beats = if self.loop_enabled && next >= self.loop_end_beats {
                next - self.loop_end_beats
            } else {
                next
            };
        } else {
            self.metronome.advance_idle();
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

        mixed.clamp(-1.0, 1.0)
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

struct DrumRuntimeTrack {
    active: bool,
    volume: f32,
    sequence: DrumSequence,
    beats_per_bar: u32,
    last_step: Option<usize>,
}

impl DrumRuntimeTrack {
    fn new(active: bool, volume: f32, sequence: DrumSequence, beats_per_bar: u32) -> Self {
        Self {
            active,
            volume,
            sequence,
            beats_per_bar,
            last_step: None,
        }
    }

    fn collect_triggers(
        &mut self,
        previous_beat: f64,
        next_beat: f64,
        loop_enabled: bool,
        loop_end_beats: f64,
        out: &mut Vec<DrumVoice>,
    ) {
        if !self.active {
            return;
        }

        let step_count = self.sequence.total_steps(self.beats_per_bar).max(1);
        let steps_per_beat = self.sequence.subdivision.steps_per_beat() as f64;
        let previous_step = (previous_beat * steps_per_beat).floor().max(0.0) as usize;
        let next_step = (next_beat * steps_per_beat).floor().max(0.0) as usize;

        if loop_enabled && next_beat >= loop_end_beats {
            self.trigger_range(previous_step.saturating_add(1), step_count.saturating_sub(1), out);
            self.trigger_range(0, (next_beat - loop_end_beats).mul_add(steps_per_beat, 0.0).floor() as usize, out);
            self.last_step = Some(0);
        } else if Some(next_step) != self.last_step {
            self.trigger_range(previous_step.saturating_add(1), next_step, out);
            self.last_step = Some(next_step);
        }
    }

    fn trigger_range(&self, start: usize, end: usize, out: &mut Vec<DrumVoice>) {
        if end < start {
            return;
        }
        let total_steps = self.sequence.total_steps(self.beats_per_bar).max(1);
        for step in start..=end {
            let normalized = step % total_steps;
            for (lane_index, lane) in self.sequence.lanes.iter().enumerate() {
                if lane.steps.get(normalized).copied().unwrap_or(false) {
                    out.push(DrumVoice::new(lane_index, lane, self.volume));
                }
            }
        }
    }
}

#[derive(Clone)]
struct ClipRuntime {
    start_beat: f64,
    length_beats: f64,
    source_offset_beats: f64,
    loop_count: f64,
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
            volume: clip.volume,
            samples: clip.samples,
        }
    }

    fn sample_at(&mut self, playhead_beats: f64, playing: bool, bpm: f64, sample_rate: f64) -> f32 {
        let span_beats = (self.length_beats.max(0.25) * self.loop_count.max(0.25)).max(0.25);
        if !playing || playhead_beats < self.start_beat || playhead_beats >= self.start_beat + span_beats {
            return 0.0;
        }
        let relative = (playhead_beats - self.start_beat).rem_euclid(self.length_beats.max(0.25));
        let seconds = (self.source_offset_beats + relative) * 60.0 / bpm.max(1.0);
        let sample_index = (seconds * sample_rate).floor() as usize;
        self.samples.get(sample_index).copied().unwrap_or(0.0) * self.volume
    }
}

struct DrumVoice {
    kind: DrumVoiceKind,
    age: usize,
    phase: f32,
    volume: f32,
    noise: u32,
    done: bool,
}

enum DrumVoiceKind {
    Kick,
    Snare,
    ClosedHat,
    OpenHat,
    Clap,
}

impl DrumVoice {
    fn new(lane_index: usize, lane: &DrumLane, volume: f32) -> Self {
        let lower = lane.name.to_ascii_lowercase();
        let kind = if lower.contains("kick") || lane_index == 0 {
            DrumVoiceKind::Kick
        } else if lower.contains("snare") || lane_index == 1 {
            DrumVoiceKind::Snare
        } else if lower.contains("open") {
            DrumVoiceKind::OpenHat
        } else if lower.contains("clap") {
            DrumVoiceKind::Clap
        } else {
            DrumVoiceKind::ClosedHat
        };

        Self {
            kind,
            age: 0,
            phase: 0.0,
            volume,
            noise: 0x1234_5678 ^ lane_index as u32,
            done: false,
        }
    }

    fn next_sample(&mut self, sample_rate: f32) -> f32 {
        let t = self.age as f32 / sample_rate;
        self.age += 1;

        let sample = match self.kind {
            DrumVoiceKind::Kick => {
                if t > 0.35 {
                    self.done = true;
                    0.0
                } else {
                    let freq = 150.0 * (1.0 - (t * 4.0).min(0.85)) + 35.0;
                    self.phase += std::f32::consts::TAU * freq / sample_rate;
                    let env = (1.0 - t / 0.35).powf(3.0);
                    self.phase.sin() * env
                }
            }
            DrumVoiceKind::Snare => {
                if t > 0.24 {
                    self.done = true;
                    0.0
                } else {
                    let env = (1.0 - t / 0.24).powf(2.5);
                    self.phase += std::f32::consts::TAU * 180.0 / sample_rate;
                    (self.rand() * 0.8 + self.phase.sin() * 0.2) * env
                }
            }
            DrumVoiceKind::ClosedHat => {
                if t > 0.08 {
                    self.done = true;
                    0.0
                } else {
                    self.rand() * (1.0 - t / 0.08).powf(2.0) * 0.7
                }
            }
            DrumVoiceKind::OpenHat => {
                if t > 0.22 {
                    self.done = true;
                    0.0
                } else {
                    self.rand() * (1.0 - t / 0.22).powf(2.2) * 0.55
                }
            }
            DrumVoiceKind::Clap => {
                if t > 0.18 {
                    self.done = true;
                    0.0
                } else {
                    let burst = if (t > 0.0 && t < 0.015) || (t > 0.02 && t < 0.035) || (t > 0.04 && t < 0.06) {
                        1.0
                    } else {
                        0.4
                    };
                    self.rand() * (1.0 - t / 0.18).powf(2.0) * burst * 0.8
                }
            }
        };

        (sample * self.volume).clamp(-1.0, 1.0)
    }

    fn rand(&mut self) -> f32 {
        self.noise = self.noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
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
    target_len: usize,
    max_len: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl AudioBuffer {
    fn new(target_len: usize, max_len: usize) -> Self {
        let samples = (0..max_len)
            .map(|_| AtomicU32::new(0.0f32.to_bits()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            samples,
            target_len,
            max_len,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn push(&self, sample: f32) {
        let mut head = self.head.load(Ordering::Relaxed);
        let mut tail = self.tail.load(Ordering::Acquire);

        if head.wrapping_sub(tail) >= self.max_len {
            tail = tail.wrapping_add(1);
            self.tail.store(tail, Ordering::Release);
        }

        let slot = head % self.max_len;
        self.samples[slot].store(sample.to_bits(), Ordering::Relaxed);
        head = head.wrapping_add(1);
        self.head.store(head, Ordering::Release);
    }

    fn pop(&self) -> f32 {
        let head = self.head.load(Ordering::Acquire);
        let mut tail = self.tail.load(Ordering::Relaxed);
        let available = head.wrapping_sub(tail);

        if available == 0 {
            return 0.0;
        }

        if available > self.target_len {
            tail = head.wrapping_sub(self.target_len);
        }

        let slot = tail % self.max_len;
        let sample = f32::from_bits(self.samples[slot].load(Ordering::Relaxed));
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        sample
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
            let preferred = TARGET_BUFFER_FRAMES as u32;
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
