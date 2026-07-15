use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::audio::{self, AudioSample, DrumSampleKit, EngineConfig, EngineHandle, PlaybackClip, PlaybackDrumClip, PlaybackMetronome, PlaybackState, SamplePreview, SequencerPreview};
use crate::content::{ContentPaths, ContentRegistry, PatternKind};
use crate::pedals::{PedalKind, PedalSpec};
use crate::project::{
    self, AudioClip, DrumSequence, MetronomeMode, Project, ProjectSummary, SequencerSubdivision, Track,
    TrackKind,
};

const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);
const LEVEL_DECAY: f32 = 0.75;
pub(crate) const VOLUME_STEP: f32 = 0.05;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    MainMenu,
    Session,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipSelection {
    pub track_index: usize,
    pub clip_index: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct PendingRecord {
    pub track_index: usize,
    pub start_beat: f32,
    pub beats_left: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct ActiveRecording {
    pub track_index: usize,
    pub start_beat: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SequencerTarget {
    New { track_index: usize },
    Edit(ClipSelection),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SequencerKeymapMode {
    DrumKit,
    Asdf,
    Custom,
}

impl SequencerKeymapMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DrumKit => "DrumKit",
            Self::Asdf => "ASDF",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum SequencerPadKey {
    Q,
    W,
    E,
    R,
    T,
    Y,
    U,
    I,
    O,
    P,
    A,
    S,
    D,
    F,
    G,
    H,
    J,
    K,
    L,
    Z,
    X,
    C,
    V,
    B,
    N,
    M,
}

impl SequencerPadKey {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Q => "Q",
            Self::W => "W",
            Self::E => "E",
            Self::R => "R",
            Self::T => "T",
            Self::Y => "Y",
            Self::U => "U",
            Self::I => "I",
            Self::O => "O",
            Self::P => "P",
            Self::A => "A",
            Self::S => "S",
            Self::D => "D",
            Self::F => "F",
            Self::G => "G",
            Self::H => "H",
            Self::J => "J",
            Self::K => "K",
            Self::L => "L",
            Self::Z => "Z",
            Self::X => "X",
            Self::C => "C",
            Self::V => "V",
            Self::B => "B",
            Self::N => "N",
            Self::M => "M",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleBrowserFilter {
    All,
    Kick,
    Snare,
    Hat,
    Tom,
    Cymbal,
    Perc,
    Fx,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternBrowserFilter {
    Loops,
    Fills,
    MyPatterns,
}

impl PatternBrowserFilter {
    pub(crate) fn matches(self, kind: PatternKind, user_owned: bool) -> bool {
        match self {
            Self::Loops => kind == PatternKind::Loop && !user_owned,
            Self::Fills => kind == PatternKind::Fill && !user_owned,
            Self::MyPatterns => user_owned,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Loops => "loops",
            Self::Fills => "fills",
            Self::MyPatterns => "my-patterns",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternApplyMode {
    Replace,
    Append,
    Overlay,
}

impl SampleBrowserFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Kick => "Kicks",
            Self::Snare => "Snares",
            Self::Hat => "Hats",
            Self::Tom => "Toms",
            Self::Cymbal => "Cymbals",
            Self::Perc => "Perc",
            Self::Fx => "FX",
        }
    }
}

#[derive(Clone)]
pub(crate) struct SampleBrowserEntry {
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) folder: String,
    pub(crate) category: SampleBrowserFilter,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SequencerLaneRole {
    Kick,
    Snare,
    Cymbal,
    Tom,
    Perc,
}

pub struct App {
    pub(crate) screen: Screen,
    pub(crate) input_devices: Vec<String>,
    pub(crate) output_devices: Vec<String>,
    pub(crate) default_input: Option<usize>,
    pub(crate) default_output: Option<usize>,
    pub(crate) project: Option<Project>,
    pub(crate) selected_track: usize,
    pub(crate) selected_clip: Option<ClipSelection>,
    pub(crate) status: String,
    pub(crate) recent_projects: Vec<ProjectSummary>,
    pub(crate) router: Option<EngineHandle>,
    pub(crate) route_enabled: bool,
    pub(crate) input_meter: f32,
    pub(crate) output_meter: f32,
    pub(crate) latency_label: String,
    pub(crate) last_device_refresh: Instant,
    pub(crate) is_playing: bool,
    pub(crate) playhead_beats: f32,
    pub(crate) last_transport_tick: Instant,
    pub(crate) transport_popup_open: bool,
    pub(crate) metronome_popup_open: bool,
    pub(crate) sequencer_popup_open: bool,
    pub(crate) sequencer_target: Option<SequencerTarget>,
    pub(crate) sequencer_draft: DrumSequence,
    pub(crate) sequencer_preview_playing: bool,
    pub(crate) sequencer_preview_beats: f32,
    pub(crate) sequencer_audition_lanes: Vec<usize>,
    pub(crate) sequencer_audition_nonce: u64,
    pub(crate) sequencer_preview_restart_nonce: u64,
    pub(crate) sequencer_record_armed: bool,
    pub(crate) sequencer_record_count_in_beats: u32,
    pub(crate) sequencer_count_in_remaining_beats: Option<f32>,
    pub(crate) sequencer_keymap_mode: SequencerKeymapMode,
    pub(crate) sequencer_lane_key_bindings: Vec<Option<SequencerPadKey>>,
    pub(crate) sequencer_key_overrides: HashSet<usize>,
    pub(crate) sequencer_last_recorded_step_per_lane: Vec<Option<usize>>,
    pub(crate) sequencer_drag_paint_mode: Option<bool>,
    pub(crate) sequencer_text_input_active: bool,
    pub(crate) pattern_browser_open: bool,
    pub(crate) pattern_browser_filter: PatternBrowserFilter,
    pub(crate) pattern_browser_query: String,
    pub(crate) pattern_show_all: bool,
    pub(crate) user_pattern_name: String,
    pub(crate) user_pattern_kind: PatternKind,
    pub(crate) sequencer_library_preview: Option<DrumSequence>,
    pub(crate) sequencer_library_preview_id: Option<String>,
    pub(crate) sample_browser_open: bool,
    pub(crate) sample_browser_target_lane: Option<usize>,
    pub(crate) sample_browser_add_variant: bool,
    pub(crate) sample_browser_dir: PathBuf,
    pub(crate) sample_browser_query: String,
    pub(crate) sample_browser_filter: SampleBrowserFilter,
    pub(crate) sample_browser_entries: Vec<SampleBrowserEntry>,
    pub(crate) sample_browser_selected_row: usize,
    pub(crate) sample_browser_scroll_to_selected: bool,
    pub(crate) sample_browser_dirty: bool,
    sample_browser_scan: Option<Receiver<Vec<SampleBrowserEntry>>>,
    pub(crate) content_registry: ContentRegistry,
    pub(crate) sample_preview_nonce: u64,
    pub(crate) sample_preview_path: Option<String>,
    sample_preview_ends_at: Option<Instant>,
    pub(crate) input_popup_open: bool,
    pub(crate) pedal_settings_open: Option<usize>,
    pub(crate) pedalboard_ratio: f32,
    pub(crate) reset_session_layout: bool,
    pub(crate) rename_project_popup_open: bool,
    pub(crate) rename_project_draft: String,
    pub(crate) rename_project_error: Option<String>,
    pub(crate) pending_record: Option<PendingRecord>,
    pub(crate) active_recording: Option<ActiveRecording>,
    pub(crate) metronome_flash: f32,
    pub(crate) last_metronome_beat: i64,
    pub(crate) audio_preview_cache: HashMap<String, Vec<f32>>,
    decoded_sample_cache: RefCell<HashMap<String, Arc<AudioSample>>>,
    pub(crate) drum_sample_kit: Arc<DrumSampleKit>,
}

impl App {
    pub fn new() -> Result<Self> {
        let inventory = audio::discover_devices()?;
        let content_registry = ContentRegistry::discover()?;
        let sample_browser_dir = content_registry.paths().kits.clone();
        let drum_sample_kit = Arc::new(load_default_drum_sample_kit(content_registry.paths()));
        Ok(Self {
            screen: Screen::MainMenu,
            input_devices: inventory.inputs,
            output_devices: inventory.outputs,
            default_input: inventory.default_input,
            default_output: inventory.default_output,
            project: None,
            selected_track: 0,
            selected_clip: None,
            status: "Create a project or open a recent session".to_string(),
            recent_projects: project::list_recent_projects().unwrap_or_default(),
            router: None,
            route_enabled: false,
            input_meter: 0.0,
            output_meter: 0.0,
            latency_label: "not running".to_string(),
            last_device_refresh: Instant::now(),
            is_playing: false,
            playhead_beats: 0.0,
            last_transport_tick: Instant::now(),
            transport_popup_open: false,
            metronome_popup_open: false,
            sequencer_popup_open: false,
            sequencer_target: None,
            sequencer_draft: DrumSequence::default(),
            sequencer_preview_playing: false,
            sequencer_preview_beats: 0.0,
            sequencer_audition_lanes: Vec::new(),
            sequencer_audition_nonce: 0,
            sequencer_preview_restart_nonce: 0,
            sequencer_record_armed: false,
            sequencer_record_count_in_beats: 4,
            sequencer_count_in_remaining_beats: None,
            sequencer_keymap_mode: SequencerKeymapMode::DrumKit,
            sequencer_lane_key_bindings: Vec::new(),
            sequencer_key_overrides: HashSet::new(),
            sequencer_last_recorded_step_per_lane: Vec::new(),
            sequencer_drag_paint_mode: None,
            sequencer_text_input_active: false,
            pattern_browser_open: false,
            pattern_browser_filter: PatternBrowserFilter::Loops,
            pattern_browser_query: String::new(),
            pattern_show_all: false,
            user_pattern_name: "My Pattern".to_string(),
            user_pattern_kind: PatternKind::Loop,
            sequencer_library_preview: None,
            sequencer_library_preview_id: None,
            sample_browser_open: false,
            sample_browser_target_lane: None,
            sample_browser_add_variant: false,
            sample_browser_dir,
            sample_browser_query: String::new(),
            sample_browser_filter: SampleBrowserFilter::All,
            sample_browser_entries: Vec::new(),
            sample_browser_selected_row: 0,
            sample_browser_scroll_to_selected: false,
            sample_browser_dirty: true,
            sample_browser_scan: None,
            content_registry,
            sample_preview_nonce: 0,
            sample_preview_path: None,
            sample_preview_ends_at: None,
            input_popup_open: false,
            pedal_settings_open: None,
            pedalboard_ratio: 0.32,
            reset_session_layout: false,
            rename_project_popup_open: false,
            rename_project_draft: String::new(),
            rename_project_error: None,
            pending_record: None,
            active_recording: None,
            metronome_flash: 0.0,
            last_metronome_beat: -1,
            audio_preview_cache: HashMap::new(),
            decoded_sample_cache: RefCell::new(HashMap::new()),
            drum_sample_kit,
        })
    }

    pub fn refresh(&mut self) {
        if self.sample_preview_ends_at.is_some_and(|ends_at| Instant::now() >= ends_at) {
            self.sample_preview_path = None;
            self.sample_preview_ends_at = None;
        }
        if self.last_device_refresh.elapsed() >= DEVICE_REFRESH_INTERVAL {
            if let Err(err) = self.refresh_devices() {
                self.status = format!("Device refresh failed: {err:#}");
            }
        }

        if let Some(router) = self.router.as_ref() {
            let meters = router.meters();
            self.input_meter = meters.input_peak.max(self.input_meter * LEVEL_DECAY);
            self.output_meter = meters.output_peak.max(self.output_meter * LEVEL_DECAY);
            self.latency_label = router.latency_label();
            if self.is_playing {
                self.playhead_beats = router.current_playhead_beats() as f32;
            }
            if self.sequencer_popup_open {
                self.sequencer_preview_beats = router.current_preview_beats() as f32;
            }

            if let Some(error) = router.take_error() {
                self.status = error;
                let _ = self.stop_router();
            }
        } else {
            self.input_meter *= LEVEL_DECAY;
            self.output_meter *= LEVEL_DECAY;
            self.latency_label = "not running".to_string();
        }

        self.advance_transport();

        self.metronome_flash *= 0.86;
    }

    fn advance_transport(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_transport_tick);
        self.last_transport_tick = now;

        let bpm = self.project.as_ref().map(|project| project.transport.bpm).unwrap_or(120) as f32;
        let beats_advanced = elapsed.as_secs_f32() * bpm / 60.0;
        if beats_advanced <= 0.0 {
            return;
        }

        if self.is_playing && self.router.is_none() {
            self.playhead_beats += beats_advanced;
            let loop_end = self.loop_end_beats();
            if self.project.as_ref().map(|project| project.transport.loop_enabled).unwrap_or(false) && loop_end > 0.0 {
                while self.playhead_beats >= loop_end {
                    self.playhead_beats -= loop_end;
                }
            }
        }

        let should_tick = self.should_tick_metronome();
        if should_tick {
            let beat_index = self.playhead_beats.floor() as i64;
            if beat_index != self.last_metronome_beat {
                self.last_metronome_beat = beat_index;
                self.metronome_flash = if self.current_bar_beat() == 0 { 1.0 } else { 0.65 };
            }
        } else {
            self.last_metronome_beat = self.playhead_beats.floor() as i64;
        }

        if let Some(mut pending) = self.pending_record.take() {
            pending.beats_left -= beats_advanced;
            if pending.beats_left <= 0.0 {
                self.active_recording = Some(ActiveRecording {
                    track_index: pending.track_index,
                    start_beat: pending.start_beat,
                });
                self.is_playing = true;
                self.playhead_beats = pending.start_beat;
                self.status = format!("Recording {}", self.track_name(pending.track_index));
                if let Some(router) = self.router.as_ref() {
                    let _ = router.begin_recording();
                }
                if let Err(err) = self.sync_router() {
                    self.status = format!("{err:#}");
                }
            } else {
                self.pending_record = Some(pending);
            }
        }

        if let Some(mut beats_left) = self.sequencer_count_in_remaining_beats.take() {
            beats_left -= beats_advanced;
            if beats_left > 0.0 {
                self.sequencer_count_in_remaining_beats = Some(beats_left);
            } else {
                self.sequencer_count_in_remaining_beats = None;
                for slot in &mut self.sequencer_last_recorded_step_per_lane {
                    *slot = None;
                }
            }
        }
    }

    fn should_tick_metronome(&self) -> bool {
        let Some(project) = self.project.as_ref() else {
            return false;
        };
        match project.metronome.mode {
            MetronomeMode::Off => false,
            MetronomeMode::On => self.is_playing || self.pending_record.is_some(),
            MetronomeMode::Always => true,
        }
    }

    pub(crate) fn stop_router(&mut self) -> Result<()> {
        if let Some(router) = self.router.take() {
            router.stop()?;
        }
        self.input_meter = 0.0;
        self.output_meter = 0.0;
        self.latency_label = "not running".to_string();
        if let Some(project) = self.project.as_mut() {
            project.input.volume = project.input.volume.clamp(0.0, 1.0);
        }
        Ok(())
    }

    pub(crate) fn create_new_project(&mut self) -> Result<()> {
        self.decoded_sample_cache.borrow_mut().clear();
        let name = format!("Jam {}", self.recent_projects.len() + 1);
        let mut project = Project::new_default(name, self.default_input_name(), self.default_output_name());
        project::save_project(&mut project)?;
        self.project = Some(project);
        self.selected_track = 1;
        self.selected_clip = None;
        self.playhead_beats = 0.0;
        self.screen = Screen::Session;
        self.reset_session_layout = true;
        self.status = "Created a new project".to_string();
        self.recent_projects = project::list_recent_projects().unwrap_or_default();
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn open_project(&mut self, path: PathBuf) -> Result<()> {
        self.decoded_sample_cache.borrow_mut().clear();
        let mut project = project::load_project(&path)?;
        project.ensure_invariants();
        self.project = Some(project);
        self.selected_track = 0;
        self.selected_clip = None;
        self.playhead_beats = 0.0;
        self.is_playing = false;
        self.pending_record = None;
        self.active_recording = None;
        self.screen = Screen::Session;
        self.reset_session_layout = true;
        self.repair_project_devices()?;
        self.status = format!("Opened {}", short_path(&path));
        Ok(())
    }

    pub(crate) fn save_project(&mut self) -> Result<()> {
        let Some(project) = self.project.as_mut() else {
            return Ok(());
        };
        project.ensure_invariants();
        project::save_project(project)?;
        self.status = format!("Saved {}", project.name);
        self.recent_projects = project::list_recent_projects().unwrap_or_default();
        Ok(())
    }

    pub(crate) fn open_rename_project_popup(&mut self) {
        if let Some(project) = self.project.as_ref() {
            self.rename_project_draft = project.name.clone();
            self.rename_project_error = None;
            self.rename_project_popup_open = true;
        }
    }

    pub(crate) fn rename_project_name_available(&self) -> bool {
        self.project
            .as_ref()
            .and_then(|project| project::project_name_available(project.path.as_deref(), &self.rename_project_draft).ok())
            .unwrap_or(false)
    }

    pub(crate) fn confirm_rename_project(&mut self) {
        let result = match self.project.as_mut() {
            Some(project) => project::rename_project(project, &self.rename_project_draft),
            None => return,
        };

        match result {
            Ok(()) => {
                self.rename_project_popup_open = false;
                self.rename_project_error = None;
                self.status = format!("Renamed project to {}", self.rename_project_draft.trim());
                self.recent_projects = project::list_recent_projects().unwrap_or_default();
            }
            Err(err) => {
                self.rename_project_error = Some(format!("{err:#}"));
            }
        }
    }

    pub(crate) fn delete_current_project(&mut self) {
        let Some(project) = self.project.take() else {
            return;
        };

        if let Err(err) = self.stop_router() {
            self.status = format!("{err:#}");
            self.project = Some(project);
            return;
        }

        match project::delete_project(&project) {
            Ok(()) => {
                self.selected_track = 0;
                self.selected_clip = None;
                self.is_playing = false;
                self.playhead_beats = 0.0;
                self.pending_record = None;
                self.active_recording = None;
                self.screen = Screen::MainMenu;
                self.status = format!("Deleted {}", project.name);
                self.recent_projects = project::list_recent_projects().unwrap_or_default();
            }
            Err(err) => {
                self.status = format!("{err:#}");
                self.project = Some(project);
            }
        }
    }

    pub(crate) fn refresh_devices(&mut self) -> Result<()> {
        self.last_device_refresh = Instant::now();
        let inventory = audio::discover_devices()?;
        let changed = inventory.inputs != self.input_devices
            || inventory.outputs != self.output_devices
            || inventory.default_input != self.default_input
            || inventory.default_output != self.default_output;
        self.input_devices = inventory.inputs;
        self.output_devices = inventory.outputs;
        self.default_input = inventory.default_input;
        self.default_output = inventory.default_output;
        if changed {
            self.repair_project_devices()?;
        }
        Ok(())
    }

    fn repair_project_devices(&mut self) -> Result<()> {
        if self.active_recording.is_some() {
            return Ok(());
        }
        let Some(project) = self.project.as_mut() else {
            return Ok(());
        };

        project.output_device = pick_device_name(&self.output_devices, project.output_device.as_deref(), self.default_output).map(str::to_string);
        project.input.device = pick_device_name(&self.input_devices, project.input.device.as_deref(), self.default_input).map(str::to_string);
        for track in &mut project.tracks {
            if track.kind == TrackKind::Audio {
                track.input_device = pick_device_name(&self.input_devices, track.input_device.as_deref(), self.default_input).map(str::to_string);
            }
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn sync_router(&mut self) -> Result<()> {
        let Some((input, output, config)) = self.current_route_config() else {
            self.stop_router()?;
            return Ok(());
        };

        let same_devices = self
            .router
            .as_ref()
            .map(|router| router.input_name() == input.as_deref() && router.output_name() == output)
            .unwrap_or(false);

        if same_devices {
            if let Some(router) = self.router.as_ref() {
                router.update_config(config)?;
            }
        } else {
            self.stop_router()?;
            let router = EngineHandle::start(input.as_deref(), &output, config)?;
            self.latency_label = router.latency_label();
            self.router = Some(router);
        }

        Ok(())
    }

    fn current_route_config(&self) -> Option<(Option<String>, String, EngineConfig)> {
        let project = self.project.as_ref()?;
        let output = project.output_device.clone()?;
        Some((project.input.device.clone(), output, self.build_engine_config()?))
    }

    pub(crate) fn route_enabled(&self) -> bool {
        self.route_enabled
    }

    pub(crate) fn toggle_route(&mut self) -> Result<()> {
        self.route_enabled = !self.route_enabled;
        self.sync_router()?;
        self.status = if self.route_enabled {
            "Routing enabled".to_string()
        } else {
            "Routing disabled".to_string()
        };
        if self.route_enabled && self.project.as_ref().and_then(|project| project.input.device.as_ref()).is_none() {
            self.status = "Routing enabled, but no input device is selected".to_string();
        }
        Ok(())
    }

    pub(crate) fn set_output_device(&mut self, device: Option<String>) -> Result<()> {
        if self.active_recording.is_some()
            && self.project.as_ref().and_then(|project| project.output_device.as_ref()) != device.as_ref()
        {
            anyhow::bail!("Stop recording before changing the output device");
        }
        if let Some(project) = self.project.as_mut() {
            project.output_device = device;
            project.dirty = true;
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn set_input_device(&mut self, device: Option<String>) -> Result<()> {
        if self.active_recording.is_some()
            && self.project.as_ref().and_then(|project| project.input.device.as_ref()) != device.as_ref()
        {
            anyhow::bail!("Stop recording before changing the input device");
        }
        if let Some(project) = self.project.as_mut() {
            project.input.device = device.clone();
            for track in &mut project.tracks {
                if track.kind == TrackKind::Audio && track.armed {
                    track.input_device = device.clone();
                }
            }
            project.dirty = true;
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn adjust_input_volume(&mut self, delta: f32) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            project.input.volume = (project.input.volume + delta).clamp(0.0, 1.0);
            project.dirty = true;
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn set_input_volume(&mut self, value: f32) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            project.input.volume = value.clamp(0.0, 1.0);
            project.dirty = true;
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn toggle_playback(&mut self) {
        if self.is_playing {
            self.stop_playback();
        } else {
            self.is_playing = true;
            self.last_transport_tick = Instant::now();
            self.status = "Playback started".to_string();
            if let Err(err) = self.sync_router() {
                self.status = format!("{err:#}");
            }
        }
    }

    pub(crate) fn stop_playback(&mut self) {
        self.is_playing = false;
        self.finalize_recording();
        self.pending_record = None;
        self.status = "Playback stopped".to_string();
        if let Err(err) = self.sync_router() {
            self.status = format!("{err:#}");
        }
    }

    fn finalize_recording(&mut self) {
        let Some(recording) = self.active_recording.take() else {
            return;
        };
        let take = self.router.as_ref().and_then(|router| router.take_recording());
        let end = self.playhead_beats.max(recording.start_beat + 0.25);
        let title = format!("Take {}", self.take_count(recording.track_index) + 1);
        let file_path = take
            .as_ref()
            .and_then(|take| self.write_recorded_take(recording.track_index, &title, take).ok());
        let clip = AudioClip {
            start_beat: recording.start_beat,
            length_beats: end - recording.start_beat,
            source_track: recording.track_index,
            title,
            source_offset_beats: 0.0,
            loop_count: 1.0,
            drum_sequence: None,
            sample_rate_hz: take.as_ref().map(|take| take.sample_rate).unwrap_or(44_100),
            file_path,
        };
        if let Some(track) = self.project.as_mut().and_then(|project| project.tracks.get_mut(recording.track_index)) {
            if track.overwrite {
                track.clips.retain(|existing| existing.end_beat() <= clip.start_beat || existing.start_beat >= clip.end_beat());
            }
            track.clips.push(clip);
            track.clips.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
        }
        self.mark_dirty();
        let _ = self.repair_project_devices();
    }

    fn take_count(&self, track_index: usize) -> usize {
        self.project
            .as_ref()
            .and_then(|project| project.tracks.get(track_index))
            .map(|track| track.clips.len())
            .unwrap_or(0)
    }

    pub(crate) fn set_playhead(&mut self, beat: f32) {
        self.playhead_beats = beat.clamp(0.0, self.max_timeline_beats());
        let _ = self.sync_router();
    }

    pub(crate) fn jump_playhead_to_previous_anchor(&mut self) {
        let target = match self.selected_clip_ref() {
            Some(clip) if (self.playhead_beats - clip.start_beat).abs() > 0.01 => clip.start_beat,
            _ => 0.0,
        };
        self.set_playhead(target);
    }

    pub(crate) fn jump_playhead_to_next_anchor(&mut self) {
        let song_end = self.max_timeline_beats();
        let target = match self.selected_clip_ref() {
            Some(clip) if (self.playhead_beats - clip.end_beat()).abs() > 0.01 => clip.end_beat(),
            _ => song_end,
        };
        self.set_playhead(target);
    }

    pub(crate) fn toggle_track_mute(&mut self, index: usize) {
        let message = if let Some(project) = self.project.as_mut() {
            if let Some(track) = project.tracks.get_mut(index) {
                track.muted = !track.muted;
                project.dirty = true;
                Some(format!("{} {}", if track.muted { "Muted" } else { "Unmuted" }, track.name))
            } else {
                None
            }
        } else {
            None
        };
        if let Some(message) = message {
            self.status = message;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn toggle_track_solo(&mut self, index: usize) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        if index >= project.tracks.len() {
            return;
        }
        let next_state = !project.tracks[index].solo;
        for (track_index, track) in project.tracks.iter_mut().enumerate() {
            track.solo = next_state && track_index == index;
        }
        project.dirty = true;
        self.status = if next_state {
            format!("Soloed {}", project.tracks[index].name)
        } else {
            format!("Unsoloed {}", project.tracks[index].name)
        };
        let _ = self.sync_router();
    }

    pub(crate) fn arm_selected_track(&mut self) {
        let selected = self.selected_track;
        let Some(project) = self.project.as_mut() else {
            return;
        };
        if project.tracks.get(selected).map(|track| track.kind) != Some(TrackKind::Audio) {
            self.sequencer_popup_open = true;
            return;
        }
        for (index, track) in project.tracks.iter_mut().enumerate() {
            track.armed = index == selected;
            if index == selected {
                track.input_device = project.input.device.clone();
            }
        }

        let count_in_beats = project.tracks[selected].count_in_beats.max(1) as f32;
        let start_beat = self.playhead_beats;
        if project.tracks[selected].count_in_enabled {
            self.pending_record = Some(PendingRecord {
                track_index: selected,
                start_beat,
                beats_left: count_in_beats,
            });
            self.active_recording = None;
            self.is_playing = false;
            self.status = format!("Count-in for {}", project.tracks[selected].name);
        } else {
            self.pending_record = None;
            self.active_recording = Some(ActiveRecording {
                track_index: selected,
                start_beat,
            });
            self.is_playing = true;
            self.status = format!("Recording {}", project.tracks[selected].name);
        }
        project.dirty = true;
        self.last_transport_tick = Instant::now();
        let _ = self.sync_router();
        if self.active_recording.is_some() {
            if let Some(router) = self.router.as_ref() {
                let _ = router.begin_recording();
            }
        }
    }

    pub(crate) fn add_audio_track(&mut self) {
        let input = self.project.as_ref().and_then(|project| project.input.device.clone());
        let count = self
            .project
            .as_ref()
            .map(|project| project.tracks.iter().filter(|track| track.kind == TrackKind::Audio).count())
            .unwrap_or(0);
        if let Some(project) = self.project.as_mut() {
            project.tracks.push(Track::new_audio(count + 1, input.as_deref()));
            project.dirty = true;
            self.selected_track = project.tracks.len().saturating_sub(1);
            self.selected_clip = None;
            self.status = "Added audio track".to_string();
        }
        let _ = self.sync_router();
    }

    pub(crate) fn remove_selected_track(&mut self) {
        if self.selected_track == 0 {
            self.status = "The drum track stays at row 0".to_string();
            return;
        }
        if let Some(project) = self.project.as_mut() {
            if self.selected_track < project.tracks.len() {
                let removed = project.tracks.remove(self.selected_track);
                project.dirty = true;
                self.selected_track = self.selected_track.saturating_sub(1).min(project.tracks.len().saturating_sub(1));
                self.selected_clip = None;
                self.status = format!("Removed {}", removed.name);
            }
        }
        let _ = self.sync_router();
    }

    pub(crate) fn rename_selected_track(&mut self, name: String) {
        if let Some(track) = self.selected_track_mut() {
            track.name = name;
            self.mark_dirty();
        }
    }

    pub(crate) fn step_track_color(&mut self, delta: i32) {
        if let Some(track) = self.selected_track_mut() {
            track.color = track.color.step(delta);
            self.mark_dirty();
        }
    }

    pub(crate) fn toggle_selected_track_mute(&mut self) {
        self.toggle_track_mute(self.selected_track);
    }

    pub(crate) fn toggle_selected_track_solo(&mut self) {
        self.toggle_track_solo(self.selected_track);
    }

    pub(crate) fn set_selected_track_count_in_enabled(&mut self, enabled: bool) {
        if let Some(track) = self.selected_track_mut() {
            track.count_in_enabled = enabled;
            self.mark_dirty();
        }
    }

    pub(crate) fn adjust_selected_track_count_in_beats(&mut self, delta: i32) {
        if let Some(track) = self.selected_track_mut() {
            track.count_in_beats = (track.count_in_beats as i32 + delta).clamp(1, 8) as u32;
            self.mark_dirty();
        }
    }

    pub(crate) fn set_selected_track_overwrite(&mut self, overwrite: bool) {
        if let Some(track) = self.selected_track_mut() {
            track.overwrite = overwrite;
            self.mark_dirty();
        }
    }

    pub(crate) fn adjust_bpm(&mut self, delta: i32) {
        if let Some(project) = self.project.as_mut() {
            project.transport.bpm = (project.transport.bpm as i32 + delta * 2).clamp(40, 240) as u32;
            project.metronome.sound.bpm = project.transport.bpm as f32;
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn set_bpm(&mut self, value: u32) {
        if let Some(project) = self.project.as_mut() {
            project.transport.bpm = value.clamp(40, 240);
            project.metronome.sound.bpm = project.transport.bpm as f32;
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_beats(&mut self, delta: i32) {
        if let Some(project) = self.project.as_mut() {
            project.transport.beats_per_bar = (project.transport.beats_per_bar as i32 + delta).clamp(1, 12) as u32;
            project.ensure_invariants();
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn set_beats_per_bar(&mut self, value: u32) {
        if let Some(project) = self.project.as_mut() {
            project.transport.beats_per_bar = value.clamp(1, 12);
            project.ensure_invariants();
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_beat_unit(&mut self, delta: i32) {
        if let Some(project) = self.project.as_mut() {
            project.transport.beat_unit = (project.transport.beat_unit as i32 + delta).clamp(1, 16) as u32;
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_loop_bars(&mut self, delta: i32) {
        if let Some(project) = self.project.as_mut() {
            project.transport.loop_bars = (project.transport.loop_bars as i32 + delta).clamp(1, 32) as u32;
            project.transport.playback_bars = project.transport.playback_bars.max(project.transport.loop_bars);
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn toggle_loop_enabled(&mut self) {
        if let Some(project) = self.project.as_mut() {
            project.transport.loop_enabled = !project.transport.loop_enabled;
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn cycle_metronome_mode(&mut self, delta: i32) {
        if let Some(project) = self.project.as_mut() {
            project.metronome.mode = project.metronome.mode.step(delta);
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_metronome_param(&mut self, index: usize, delta: i32) {
        if let Some(project) = self.project.as_mut() {
            match index {
                0 => {
                    let bpm = (project.metronome.sound.bpm + delta as f32 * 2.0).clamp(40.0, 240.0);
                    project.metronome.sound.bpm = bpm;
                    project.transport.bpm = bpm.round() as u32;
                }
                1 => project.metronome.sound.accent_every = (project.metronome.sound.accent_every as i32 + delta).clamp(1, 12) as u32,
                2 => project.metronome.sound.tone_hz = (project.metronome.sound.tone_hz + delta as f32 * 50.0).clamp(120.0, 4000.0),
                _ => project.metronome.sound.volume = (project.metronome.sound.volume + delta as f32 * 0.05).clamp(0.0, 1.0),
            }
            project.dirty = true;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn add_pedal(&mut self, kind: PedalKind) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            project.pedalboard.push(PedalSpec::new(kind));
            project.dirty = true;
            self.pedal_settings_open = Some(project.pedalboard.len().saturating_sub(1));
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn remove_pedal(&mut self, index: usize) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            if index < project.pedalboard.len() {
                project.pedalboard.remove(index);
                project.dirty = true;
                self.pedal_settings_open = None;
            }
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn move_pedal(&mut self, index: usize, delta: i32) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            if index < project.pedalboard.len() {
                let next = ((index as i32 + delta).clamp(0, project.pedalboard.len().saturating_sub(1) as i32)) as usize;
                if next != index {
                    project.pedalboard.swap(index, next);
                    project.dirty = true;
                    self.pedal_settings_open = Some(next);
                }
            }
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn toggle_pedal_enabled(&mut self, index: usize) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            if let Some(pedal) = project.pedalboard.get_mut(index) {
                pedal.toggle_enabled();
                project.dirty = true;
            }
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn adjust_pedal_param(&mut self, index: usize, param_index: usize, delta: i32) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            if let Some(pedal) = project.pedalboard.get_mut(index) {
                pedal.step_param(param_index, delta);
                project.dirty = true;
            }
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn selected_track_ref(&self) -> Option<&Track> {
        self.project.as_ref()?.tracks.get(self.selected_track)
    }

    pub(crate) fn selected_track_mut(&mut self) -> Option<&mut Track> {
        self.project.as_mut()?.tracks.get_mut(self.selected_track)
    }

    pub(crate) fn select_track(&mut self, index: usize) {
        self.selected_track = index;
        if let Some(selection) = self.selected_clip {
            if selection.track_index != index {
                self.selected_clip = None;
            }
        }
    }

    pub(crate) fn select_clip(&mut self, track_index: usize, clip_index: usize) {
        self.selected_track = track_index;
        self.selected_clip = Some(ClipSelection { track_index, clip_index });
    }

    pub(crate) fn clear_clip_selection(&mut self) {
        self.selected_clip = None;
    }

    pub(crate) fn duplicate_selected_clip(&mut self) {
        let Some(selection) = self.selected_clip else {
            return;
        };
        let Some(track) = self.project.as_mut().and_then(|project| project.tracks.get_mut(selection.track_index)) else {
            return;
        };
        if let Some(clip) = track.clips.get(selection.clip_index).cloned() {
            let mut duplicate = clip.clone();
            duplicate.start_beat = clip.end_beat();
            duplicate.title = format!("{} copy", clip.title);
            track.clips.insert(selection.clip_index + 1, duplicate);
            self.selected_clip = Some(ClipSelection {
                track_index: selection.track_index,
                clip_index: selection.clip_index + 1,
            });
            self.mark_dirty();
        }
    }

    pub(crate) fn split_selected_clip(&mut self) {
        let Some(selection) = self.selected_clip else {
            return;
        };
        let playhead = self.playhead_beats;
        let Some(track) = self.project.as_mut().and_then(|project| project.tracks.get_mut(selection.track_index)) else {
            return;
        };
        if let Some(clip) = track.clips.get(selection.clip_index).cloned() {
            if playhead <= clip.start_beat + 0.25 || playhead >= clip.end_beat() - 0.25 {
                return;
            }
            let left_length = playhead - clip.start_beat;
            let right_length = clip.end_beat() - playhead;
            let source_length = clip.length_beats.max(0.25);
            track.clips[selection.clip_index].loop_count = (left_length / source_length).max(0.25);
            track.clips.insert(
                selection.clip_index + 1,
                AudioClip {
                    start_beat: playhead,
                    length_beats: source_length,
                    source_track: selection.track_index,
                    title: format!("{} B", clip.title),
                    source_offset_beats: clip.source_offset_beats + (left_length % source_length),
                    loop_count: (right_length / source_length).max(0.25),
                    drum_sequence: clip.drum_sequence.clone(),
                    sample_rate_hz: clip.sample_rate_hz,
                    file_path: clip.file_path.clone(),
                },
            );
            self.selected_clip = Some(ClipSelection {
                track_index: selection.track_index,
                clip_index: selection.clip_index + 1,
            });
            self.mark_dirty();
        }
    }

    pub(crate) fn delete_selected_clip(&mut self) {
        let Some(selection) = self.selected_clip else {
            return;
        };
        if let Some(track) = self
            .project
            .as_mut()
            .and_then(|project| project.tracks.get_mut(selection.track_index))
        {
            if selection.clip_index < track.clips.len() {
                track.clips.remove(selection.clip_index);
                self.selected_clip = None;
                self.mark_dirty();
            }
        }
    }

    pub(crate) fn nudge_selected_clip(&mut self, delta: f32, snap: bool) {
        let Some(selection) = self.selected_clip else {
            return;
        };
        if let Some(clip) = self
            .project
            .as_mut()
            .and_then(|project| project.tracks.get_mut(selection.track_index))
            .and_then(|track| track.clips.get_mut(selection.clip_index))
        {
            let mut next = (clip.start_beat + delta).max(0.0);
            if snap {
                next = next.round();
            }
            clip.start_beat = next;
            self.mark_dirty();
        }
    }

    pub(crate) fn set_clip_start(&mut self, track_index: usize, clip_index: usize, start_beat: f32, snap: bool) {
        if let Some(clip) = self
            .project
            .as_mut()
            .and_then(|project| project.tracks.get_mut(track_index))
            .and_then(|track| track.clips.get_mut(clip_index))
        {
            clip.start_beat = if snap { start_beat.max(0.0).round() } else { start_beat.max(0.0) };
            self.mark_dirty();
        }
    }

    pub(crate) fn trim_clip_left(&mut self, track_index: usize, clip_index: usize, start_beat: f32, snap: bool) {
        if let Some(clip) = self
            .project
            .as_mut()
            .and_then(|project| project.tracks.get_mut(track_index))
            .and_then(|track| track.clips.get_mut(clip_index))
        {
            let end = clip.end_beat();
            let start = if snap {
                start_beat.max(0.0).round()
            } else {
                start_beat.max(0.0)
            };
            let clamped_start = start.min(end - 0.25);
            clip.source_offset_beats += clamped_start - clip.start_beat;
            clip.start_beat = clamped_start;
            clip.length_beats = (end - clamped_start).max(0.25);
            self.mark_dirty();
        }
    }

    pub(crate) fn set_clip_end(&mut self, track_index: usize, clip_index: usize, end_beat: f32, snap: bool) {
        if let Some(clip) = self
            .project
            .as_mut()
            .and_then(|project| project.tracks.get_mut(track_index))
            .and_then(|track| track.clips.get_mut(clip_index))
        {
            let end = if snap { end_beat.max(clip.start_beat + 0.25).round() } else { end_beat.max(clip.start_beat + 0.25) };
            clip.loop_count = ((end - clip.start_beat) / clip.length_beats.max(0.25)).max(0.25);
            self.mark_dirty();
        }
    }

    pub(crate) fn set_selected_clip_loop_count(&mut self, loop_count: f32) {
        let Some(selection) = self.selected_clip else {
            return;
        };
        if let Some(clip) = self
            .project
            .as_mut()
            .and_then(|project| project.tracks.get_mut(selection.track_index))
            .and_then(|track| track.clips.get_mut(selection.clip_index))
        {
            clip.loop_count = loop_count.clamp(0.25, 64.0);
            self.mark_dirty();
        }
    }

    pub(crate) fn active_recording_preview(&self, track_index: usize) -> Option<AudioClip> {
        let recording = self.active_recording?;
        if recording.track_index != track_index {
            return None;
        }
        Some(AudioClip {
            start_beat: recording.start_beat,
            length_beats: (self.playhead_beats - recording.start_beat).max(0.25),
            source_track: track_index,
            title: "Recording".to_string(),
            source_offset_beats: 0.0,
            loop_count: 1.0,
            drum_sequence: None,
            sample_rate_hz: 44_100,
            file_path: None,
        })
    }

    pub(crate) fn pending_record_beats(&self) -> Option<f32> {
        self.pending_record.map(|pending| pending.beats_left.max(0.0))
    }

    pub(crate) fn selected_track_is_recording(&self) -> bool {
        self.pending_record
            .map(|pending| pending.track_index == self.selected_track)
            .unwrap_or(false)
            || self
                .active_recording
                .map(|recording| recording.track_index == self.selected_track)
                .unwrap_or(false)
    }

    pub(crate) fn begin_new_sequence_chunk(&mut self) {
        let Some(track) = self.selected_track_ref() else {
            return;
        };
        if track.kind != TrackKind::Drum {
            return;
        }
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        let mut draft = DrumSequence::default();
        draft.ensure_len(beats);
        self.sequencer_draft = draft;
        self.sequencer_preview_playing = false;
        self.sequencer_preview_beats = 0.0;
        self.sequencer_preview_restart_nonce = self.sequencer_preview_restart_nonce.wrapping_add(1);
        self.sequencer_audition_lanes.clear();
        self.sequencer_record_armed = false;
        self.sequencer_count_in_remaining_beats = None;
        self.sequencer_drag_paint_mode = None;
        self.pattern_browser_open = true;
        self.pattern_browser_filter = PatternBrowserFilter::Loops;
        self.sequencer_library_preview = None;
        self.sample_browser_open = false;
        self.sample_preview_path = None;
        self.sample_preview_ends_at = None;
        self.sequencer_target = Some(SequencerTarget::New {
            track_index: self.selected_track,
        });
        self.sequencer_popup_open = true;
        self.rebuild_sequencer_lane_key_bindings(false);
        let _ = self.sync_router();
    }

    pub(crate) fn edit_selected_sequence_chunk(&mut self) {
        let Some(selection) = self.selected_clip else {
            return;
        };
        let Some(sequence) = self
            .project
            .as_ref()
            .and_then(|project| project.tracks.get(selection.track_index))
            .and_then(|track| track.clips.get(selection.clip_index))
            .and_then(|clip| clip.drum_sequence.clone())
        else {
            return;
        };
        self.sequencer_draft = sequence;
        self.sequencer_preview_playing = false;
        self.sequencer_preview_beats = 0.0;
        self.sequencer_preview_restart_nonce = self.sequencer_preview_restart_nonce.wrapping_add(1);
        self.sequencer_audition_lanes.clear();
        self.sequencer_record_armed = false;
        self.sequencer_count_in_remaining_beats = None;
        self.sequencer_drag_paint_mode = None;
        self.pattern_browser_open = false;
        self.sequencer_library_preview = None;
        self.sample_browser_open = false;
        self.sample_preview_path = None;
        self.sample_preview_ends_at = None;
        self.sequencer_target = Some(SequencerTarget::Edit(selection));
        self.sequencer_popup_open = true;
        self.rebuild_sequencer_lane_key_bindings(false);
        let _ = self.sync_router();
    }

    pub(crate) fn save_sequence_chunk(&mut self) {
        let Some(target) = self.sequencer_target else {
            return;
        };
        let beats_per_bar = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.ensure_len(beats_per_bar);
        let draft = self.sequencer_draft.clone();
        let length_beats = draft.measures.max(1) as f32 * beats_per_bar as f32;

        match target {
            SequencerTarget::New { track_index } => {
                if let Some(track) = self.project.as_mut().and_then(|project| project.tracks.get_mut(track_index)) {
                    let count = track.clips.iter().filter(|clip| clip.is_drum_sequence()).count() + 1;
                    track.clips.push(AudioClip {
                        start_beat: self.playhead_beats,
                        length_beats,
                        source_track: track_index,
                        title: format!("Pattern {count}"),
                        source_offset_beats: 0.0,
                        loop_count: 1.0,
                        drum_sequence: Some(draft),
                        sample_rate_hz: 44_100,
                        file_path: None,
                    });
                    let clip_index = track.clips.len() - 1;
                    self.selected_clip = Some(ClipSelection { track_index, clip_index });
                    self.selected_track = track_index;
                }
            }
            SequencerTarget::Edit(selection) => {
                if let Some(clip) = self
                    .project
                    .as_mut()
                    .and_then(|project| project.tracks.get_mut(selection.track_index))
                    .and_then(|track| track.clips.get_mut(selection.clip_index))
                {
                    clip.drum_sequence = Some(draft);
                    clip.length_beats = length_beats;
                    self.selected_clip = Some(selection);
                }
            }
        }

        self.sequencer_popup_open = false;
        self.sequencer_target = None;
        self.sequencer_preview_playing = false;
        self.sequencer_preview_beats = 0.0;
        self.sequencer_audition_lanes.clear();
        self.sequencer_record_armed = false;
        self.sequencer_count_in_remaining_beats = None;
        self.sequencer_drag_paint_mode = None;
        self.pattern_browser_open = false;
        self.sequencer_library_preview = None;
        self.sample_preview_path = None;
        self.sample_preview_ends_at = None;
        self.mark_dirty();
        let _ = self.sync_router();
    }

    pub(crate) fn cancel_sequence_chunk(&mut self) {
        self.sequencer_popup_open = false;
        self.sequencer_target = None;
        self.sequencer_preview_playing = false;
        self.sequencer_preview_beats = 0.0;
        self.sequencer_audition_lanes.clear();
        self.sequencer_record_armed = false;
        self.sequencer_count_in_remaining_beats = None;
        self.sequencer_drag_paint_mode = None;
        self.pattern_browser_open = false;
        self.sequencer_library_preview = None;
        self.sample_preview_path = None;
        self.sample_preview_ends_at = None;
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_selected_sequence_measures(&mut self, delta: i32) {
        self.sequencer_library_preview = None;
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.measures = (self.sequencer_draft.measures as i32 + delta).clamp(1, 8) as u32;
        self.sequencer_draft.ensure_len(beats);
        let length = self.sequencer_preview_length_beats();
        self.sequencer_preview_beats = self.sequencer_preview_beats.rem_euclid(length.max(0.25));
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_selected_sequence_subdivision(&mut self, delta: i32) {
        self.sequencer_library_preview = None;
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        let next = self.sequencer_draft.subdivision.step(delta);
        normalize_pattern_subdivision(&mut self.sequencer_draft, next, beats);
        let _ = self.sync_router();
    }

    pub(crate) fn toggle_sequence_step(&mut self, lane_index: usize, step_index: usize) {
        let current = self
            .sequencer_draft
            .lanes
            .get(lane_index)
            .and_then(|lane| lane.steps.get(step_index))
            .copied()
            .unwrap_or(false);
        self.set_sequence_step_enabled(lane_index, step_index, !current, true);
    }

    pub(crate) fn set_sequence_step_enabled(&mut self, lane_index: usize, step_index: usize, enabled: bool, audition: bool) {
        self.sequencer_library_preview = None;
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.ensure_len(beats);
        let mut changed = false;
        if let Some(step) = self
            .sequencer_draft
            .lanes
            .get_mut(lane_index)
            .and_then(|lane| lane.steps.get_mut(step_index))
        {
            if *step != enabled {
                *step = enabled;
                changed = true;
            }
        }
        if audition {
            self.audition_sequence_lane(lane_index);
        }
        if changed || audition {
            let _ = self.sync_router();
        }
    }

    pub(crate) fn apply_pattern(&mut self, id: &str, mode: PatternApplyMode) {
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        let source_beats = self
            .content_registry
            .patterns()
            .iter()
            .find(|pattern| pattern.id == id)
            .map_or(beats, |pattern| pattern.beats_per_bar);
        let mut incoming = match self.content_registry.load_pattern(id) {
            Ok(sequence) => sequence,
            Err(error) => {
                self.status = format!("Pattern could not be loaded: {error:#}");
                return;
            }
        };
        adapt_pattern_meter(&mut incoming, source_beats, beats);
        match mode {
            PatternApplyMode::Replace => {
                let previous = self.sequencer_draft.clone();
                self.sequencer_draft = incoming;
                preserve_lane_samples(&previous, &mut self.sequencer_draft);
            }
            PatternApplyMode::Append => {
                if !append_pattern(&mut self.sequencer_draft, &incoming, beats) {
                    self.status = "The appended pattern would exceed the 8-bar sequence limit".to_string();
                    return;
                }
            }
            PatternApplyMode::Overlay => overlay_pattern(&mut self.sequencer_draft, &incoming, beats),
        }
        self.sequencer_draft.ensure_len(beats);
        self.sequencer_library_preview = None;
        self.rebuild_sequencer_lane_key_bindings(false);
        let _ = self.sync_router();
    }

    pub(crate) fn start_empty_sequence(&mut self) {
        self.sequencer_draft = DrumSequence::default();
        self.sequencer_library_preview = None;
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.ensure_len(beats);
        self.pattern_browser_open = false;
        self.rebuild_sequencer_lane_key_bindings(false);
        let _ = self.sync_router();
    }

    pub(crate) fn preview_library_pattern(&mut self, id: &str) {
        if self.is_library_pattern_previewing(id) {
            self.stop_library_pattern_preview();
            return;
        }
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        let source_beats = self
            .content_registry
            .patterns()
            .iter()
            .find(|pattern| pattern.id == id)
            .map_or(beats, |pattern| pattern.beats_per_bar);
        match self.content_registry.load_pattern(id) {
            Ok(mut sequence) => {
                adapt_pattern_meter(&mut sequence, source_beats, beats);
                preserve_lane_samples(&self.sequencer_draft, &mut sequence);
                self.sequencer_library_preview = Some(sequence);
                self.sequencer_library_preview_id = Some(id.to_string());
                self.sequencer_preview_beats = 0.0;
                self.sequencer_preview_restart_nonce = self.sequencer_preview_restart_nonce.wrapping_add(1);
                self.sequencer_preview_playing = true;
                let _ = self.sync_router();
            }
            Err(error) => self.status = format!("Pattern could not be previewed: {error:#}"),
        }
    }

    pub(crate) fn stop_library_pattern_preview(&mut self) {
        if self.sequencer_library_preview.take().is_some() {
            self.sequencer_library_preview_id = None;
            self.sequencer_preview_playing = false;
            self.sequencer_preview_beats = 0.0;
            self.sequencer_preview_restart_nonce = self.sequencer_preview_restart_nonce.wrapping_add(1);
            let _ = self.sync_router();
        }
    }

    pub(crate) fn is_library_pattern_previewing(&self, id: &str) -> bool {
        self.sequencer_preview_playing
            && self.sequencer_library_preview.is_some()
            && self.sequencer_library_preview_id.as_deref() == Some(id)
    }

    pub(crate) fn save_current_user_pattern(&mut self) {
        let name = self.user_pattern_name.trim();
        if name.is_empty() {
            self.status = "Enter a pattern name first".to_string();
            return;
        }
        let (bpm, beats_per_bar) = self
            .project
            .as_ref()
            .map(|project| (project.transport.bpm, project.transport.beats_per_bar))
            .unwrap_or((110, 4));
        match self
            .content_registry
            .save_user_pattern(name, &self.sequencer_draft, self.user_pattern_kind, bpm, beats_per_bar)
        {
            Ok(()) => self.status = format!("Saved {name} to My Patterns"),
            Err(error) => self.status = format!("Pattern could not be saved: {error:#}"),
        }
    }

    pub(crate) fn sequence(&self) -> Option<&DrumSequence> {
        self.sequencer_target.as_ref().map(|_| &self.sequencer_draft)
    }

    pub(crate) fn sequencer_preview_length_beats(&self) -> f32 {
        let beats_per_bar = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4) as f32;
        self.active_preview_sequence().measures.max(1) as f32 * beats_per_bar
    }

    pub(crate) fn current_sequencer_preview_beats(&self) -> f32 {
        self.sequencer_preview_beats
    }

    pub(crate) fn toggle_sequencer_preview_playback(&mut self) {
        if self.sequencer_preview_playing {
            if let Some(router) = self.router.as_ref() {
                self.sequencer_preview_beats = router.current_preview_beats() as f32;
            }
            self.sequencer_preview_playing = false;
            self.sequencer_count_in_remaining_beats = None;
        } else {
            let length = self.sequencer_preview_length_beats();
            self.sequencer_preview_beats = self.sequencer_preview_beats.rem_euclid(length.max(0.25));
            self.sequencer_preview_playing = true;
            if self.sequencer_record_armed {
                self.start_sequencer_count_in();
            }
        }
        for slot in &mut self.sequencer_last_recorded_step_per_lane {
            *slot = None;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn audition_sequence_lane(&mut self, lane_index: usize) {
        self.sequencer_audition_lanes.clear();
        self.sequencer_audition_lanes.push(lane_index);
        self.sequencer_audition_nonce = self.sequencer_audition_nonce.wrapping_add(1);
    }

    pub(crate) fn preview_sample_file(&mut self, path: String) {
        if self.sample_preview_path.as_deref() == Some(path.as_str()) {
            self.sample_preview_path = None;
            self.sample_preview_ends_at = None;
        } else {
            let Some(sample) = self.cached_audio_sample(&path) else {
                self.status = format!("Sample could not be loaded: {path}");
                return;
            };
            let duration = Duration::from_secs_f64(sample.samples.len() as f64 / sample.sample_rate_hz.max(1) as f64);
            self.sample_preview_path = Some(path);
            self.sample_preview_ends_at = Some(Instant::now() + duration);
        }
        self.sample_preview_nonce = self.sample_preview_nonce.wrapping_add(1);
        let _ = self.sync_router();
    }

    pub(crate) fn stop_sample_preview(&mut self) {
        if self.sample_preview_path.take().is_some() {
            self.sample_preview_ends_at = None;
            self.sample_preview_nonce = self.sample_preview_nonce.wrapping_add(1);
            let _ = self.sync_router();
        }
    }

    pub(crate) fn add_sequence_lane_from_sample(&mut self, path: String) {
        self.sequencer_library_preview = None;
        let title = PathBuf::from(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Sample")
            .to_string();
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        if let Some(lane_index) = self.sample_browser_target_lane.take() {
            if let Some(lane) = self.sequencer_draft.lanes.get_mut(lane_index) {
                if self.sample_browser_add_variant {
                    if !lane.sample_variants.contains(&path) && lane.sample_path.as_ref() != Some(&path) {
                        lane.sample_variants.push(path.clone());
                    }
                } else {
                    lane.sample_path = Some(path.clone());
                    lane.sample_variants.clear();
                }
            }
        } else {
            self.sequencer_draft.lanes.push(crate::project::DrumLane::new(&title, Some(path.clone())));
        }
        self.sequencer_draft.ensure_len(beats);
        self.rebuild_sequencer_lane_key_bindings(true);
        self.preview_sample_file(path);
    }

    pub(crate) fn open_sample_browser_for_lane(&mut self, lane_index: Option<usize>) {
        self.stop_library_pattern_preview();
        self.sequencer_preview_beats = 0.0;
        self.sample_browser_target_lane = lane_index;
        self.sample_browser_add_variant = false;
        self.sample_browser_open = true;
        self.pattern_browser_open = false;
        let _ = self.sync_router();
    }

    pub(crate) fn toggle_sample_browser_for_lane(&mut self, lane_index: Option<usize>) {
        if self.sample_browser_open && self.sample_browser_target_lane == lane_index {
            self.close_sample_browser();
        } else {
            self.open_sample_browser_for_lane(lane_index);
        }
    }

    pub(crate) fn close_sample_browser(&mut self) {
        self.sample_browser_open = false;
        self.sample_browser_target_lane = None;
        self.sample_browser_add_variant = false;
        self.stop_sample_preview();
    }

    pub(crate) fn remove_sequence_lane(&mut self, lane_index: usize) {
        self.sequencer_library_preview = None;
        if lane_index >= self.sequencer_draft.lanes.len() {
            return;
        }
        self.sequencer_draft.lanes.remove(lane_index);
        if lane_index < self.sequencer_lane_key_bindings.len() {
            self.sequencer_lane_key_bindings.remove(lane_index);
            self.sequencer_last_recorded_step_per_lane.remove(lane_index);
        }
        self.sequencer_key_overrides = self
            .sequencer_key_overrides
            .iter()
            .filter_map(|index| (*index != lane_index).then_some(if *index > lane_index { *index - 1 } else { *index }))
            .collect();
        self.sample_browser_target_lane = match self.sample_browser_target_lane {
            Some(index) if index == lane_index => {
                self.sample_browser_open = false;
                None
            }
            Some(index) if index > lane_index => Some(index - 1),
            target => target,
        };
        self.rebuild_sequencer_lane_key_bindings(true);
        let _ = self.sync_router();
    }

    pub(crate) fn move_sequence_lane(&mut self, lane_index: usize, delta: i32) {
        self.sequencer_library_preview = None;
        let target = (lane_index as i32 + delta).clamp(0, self.sequencer_draft.lanes.len().saturating_sub(1) as i32) as usize;
        if target == lane_index || lane_index >= self.sequencer_draft.lanes.len() {
            return;
        }
        self.sequencer_draft.lanes.swap(lane_index, target);
        self.sequencer_lane_key_bindings.swap(lane_index, target);
        self.sequencer_last_recorded_step_per_lane.swap(lane_index, target);
        let source_override = self.sequencer_key_overrides.remove(&lane_index);
        let target_override = self.sequencer_key_overrides.remove(&target);
        if source_override {
            self.sequencer_key_overrides.insert(target);
        }
        if target_override {
            self.sequencer_key_overrides.insert(lane_index);
        }
        if self.sample_browser_target_lane == Some(lane_index) {
            self.sample_browser_target_lane = Some(target);
        } else if self.sample_browser_target_lane == Some(target) {
            self.sample_browser_target_lane = Some(lane_index);
        }
        self.rebuild_sequencer_lane_key_bindings(true);
        let _ = self.sync_router();
    }

    pub(crate) fn toggle_sequence_lane_mute(&mut self, lane_index: usize) {
        self.sequencer_library_preview = None;
        if let Some(lane) = self.sequencer_draft.lanes.get_mut(lane_index) {
            lane.muted = !lane.muted;
            let _ = self.sync_router();
        }
    }

    pub(crate) fn set_sequence_lane_gain(&mut self, lane_index: usize, gain: f32) {
        self.sequencer_library_preview = None;
        if let Some(lane) = self.sequencer_draft.lanes.get_mut(lane_index) {
            lane.gain = gain.clamp(0.0, 2.0);
            let _ = self.sync_router();
        }
    }

    pub(crate) fn set_sequence_lane_role(&mut self, lane_index: usize, role: crate::project::DrumRole) {
        self.sequencer_library_preview = None;
        if let Some(lane) = self.sequencer_draft.lanes.get_mut(lane_index) {
            lane.role = role;
            if lane.name.trim().is_empty() || crate::project::DrumRole::infer(&lane.name) != role {
                lane.name = role.label().to_string();
            }
            self.rebuild_sequencer_lane_key_bindings(true);
            let _ = self.sync_router();
        }
    }

    pub(crate) fn sequencer_available_pad_keys() -> &'static [SequencerPadKey] {
        &SEQUENCER_ALL_PAD_KEYS
    }

    pub(crate) fn sequencer_lane_binding(&self, lane_index: usize) -> Option<SequencerPadKey> {
        self.sequencer_lane_key_bindings.get(lane_index).copied().flatten()
    }

    pub(crate) fn set_sequencer_keymap_mode(&mut self, mode: SequencerKeymapMode) {
        self.sequencer_keymap_mode = mode;
        self.rebuild_sequencer_lane_key_bindings(false);
    }

    pub(crate) fn set_sequencer_lane_binding(&mut self, lane_index: usize, key: Option<SequencerPadKey>) {
        self.rebuild_sequencer_lane_key_bindings(true);
        if lane_index >= self.sequencer_draft.lanes.len() {
            return;
        }
        if self.sequencer_lane_key_bindings.len() <= lane_index {
            self.sequencer_lane_key_bindings.resize(lane_index + 1, None);
        }
        if let Some(target_key) = key {
            for (index, slot) in self.sequencer_lane_key_bindings.iter_mut().enumerate() {
                if index != lane_index && *slot == Some(target_key) {
                    *slot = None;
                }
            }
            self.sequencer_lane_key_bindings[lane_index] = Some(target_key);
            self.sequencer_key_overrides.insert(lane_index);
        } else {
            self.sequencer_lane_key_bindings[lane_index] = None;
            self.sequencer_key_overrides.remove(&lane_index);
        }
        self.sequencer_keymap_mode = SequencerKeymapMode::Custom;
        self.rebuild_sequencer_lane_key_bindings(true);
    }

    pub(crate) fn toggle_sequencer_record_armed(&mut self) {
        self.sequencer_record_armed = !self.sequencer_record_armed;
        if self.sequencer_record_armed {
            if !self.sequencer_preview_playing {
                let length = self.sequencer_preview_length_beats();
                self.sequencer_preview_beats = self.sequencer_preview_beats.rem_euclid(length.max(0.25));
                self.sequencer_preview_playing = true;
            }
            self.start_sequencer_count_in();
        } else {
            self.sequencer_count_in_remaining_beats = None;
        }
        for slot in &mut self.sequencer_last_recorded_step_per_lane {
            *slot = None;
        }
        let _ = self.sync_router();
    }

    pub(crate) fn handle_sequencer_pad_keys(&mut self, keys: &[SequencerPadKey]) {
        if keys.is_empty() {
            return;
        }
        self.rebuild_sequencer_lane_key_bindings(true);
        let mut lanes = Vec::new();
        for key in keys {
            let Some(lane_index) = self
                .sequencer_lane_key_bindings
                .iter()
                .position(|binding| *binding == Some(*key))
            else {
                continue;
            };
            if !lanes.contains(&lane_index) {
                lanes.push(lane_index);
            }
        }
        if lanes.is_empty() {
            return;
        }
        self.sequencer_audition_lanes = lanes.clone();
        self.sequencer_audition_nonce = self.sequencer_audition_nonce.wrapping_add(1);
        if self.sequencer_record_armed
            && self.sequencer_preview_playing
            && self.sequencer_count_in_remaining_beats.unwrap_or(0.0) <= 0.0
        {
            for lane_index in lanes {
                self.record_step_at_current_preview(lane_index);
            }
        }
        let _ = self.sync_router();
    }

    pub(crate) fn adjust_sequencer_record_count_in_beats(&mut self, delta: i32) {
        self.sequencer_record_count_in_beats =
            (self.sequencer_record_count_in_beats as i32 + delta).clamp(1, 8) as u32;
    }

    pub(crate) fn current_sequencer_preview_step_index(&self) -> Option<usize> {
        if self.sequencer_library_preview.is_some() {
            return None;
        }
        let beats_per_bar = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        let total_steps = sequence_step_count(&self.sequencer_draft, beats_per_bar);
        if total_steps == 0 {
            return None;
        }
        let preview_length = self.sequencer_preview_length_beats().max(0.25);
        let preview_beats = self.current_sequencer_preview_beats().rem_euclid(preview_length);
        let step = (preview_beats * self.sequencer_draft.subdivision.steps_per_beat() as f32).floor() as usize % total_steps;
        Some(step)
    }

    fn active_preview_sequence(&self) -> &DrumSequence {
        self.sequencer_library_preview.as_ref().unwrap_or(&self.sequencer_draft)
    }

    pub(crate) fn poll_sample_browser_entries(&mut self) {
        if self.sample_browser_dirty && self.sample_browser_scan.is_none() {
            let root = self.sample_browser_dir.clone();
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let mut entries = Vec::new();
                collect_sample_browser_entries(&root, &root, &mut entries);
                entries.sort_by(|left, right| {
                    left.title
                        .cmp(&right.title)
                        .then_with(|| left.folder.cmp(&right.folder))
                        .then_with(|| left.path.cmp(&right.path))
                });
                let _ = sender.send(entries);
            });
            self.sample_browser_scan = Some(receiver);
        }
        let result = self.sample_browser_scan.as_ref().map(Receiver::try_recv);
        match result {
            Some(Ok(entries)) => {
                self.sample_browser_entries = entries;
                self.sample_browser_selected_row = 0;
                self.sample_browser_dirty = false;
                self.sample_browser_scan = None;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.sample_browser_dirty = false;
                self.sample_browser_scan = None;
                self.status = "Installed sounds could not be scanned".to_string();
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
    }

    pub(crate) fn mark_sample_browser_dirty(&mut self) {
        self.sample_browser_dirty = true;
    }

    pub(crate) fn sample_browser_loading(&self) -> bool {
        self.sample_browser_scan.is_some()
    }

    fn record_step_at_current_preview(&mut self, lane_index: usize) {
        let Some(step_index) = self.current_sequencer_preview_step_index() else {
            return;
        };
        if self.sequencer_last_recorded_step_per_lane.len() <= lane_index {
            self.sequencer_last_recorded_step_per_lane.resize(lane_index + 1, None);
        }
        if self
            .sequencer_last_recorded_step_per_lane
            .get(lane_index)
            .copied()
            .flatten()
            == Some(step_index)
        {
            return;
        }
        self.sequencer_last_recorded_step_per_lane[lane_index] = Some(step_index);
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.ensure_len(beats);
        if let Some(step) = self
            .sequencer_draft
            .lanes
            .get_mut(lane_index)
            .and_then(|lane| lane.steps.get_mut(step_index))
        {
            *step = true;
        }
    }

    fn start_sequencer_count_in(&mut self) {
        let beats = self.sequencer_record_count_in_beats.max(1);
        self.sequencer_count_in_remaining_beats = Some(beats as f32);
    }

    fn rebuild_sequencer_lane_key_bindings(&mut self, preserve_overrides: bool) {
        let lane_count = self.sequencer_draft.lanes.len();
        self.sequencer_lane_key_bindings.resize(lane_count, None);
        self.sequencer_last_recorded_step_per_lane.resize(lane_count, None);
        self.sequencer_key_overrides.retain(|index| *index < lane_count);
        if !preserve_overrides || self.sequencer_keymap_mode != SequencerKeymapMode::Custom {
            self.sequencer_key_overrides.clear();
        }
        let mut used = HashSet::new();
        if preserve_overrides {
            let overridden: Vec<usize> = self.sequencer_key_overrides.iter().copied().collect();
            for lane_index in overridden {
                let Some(binding) = self.sequencer_lane_key_bindings.get(lane_index).copied().flatten() else {
                    self.sequencer_key_overrides.remove(&lane_index);
                    continue;
                };
                if !used.insert(binding) {
                    self.sequencer_lane_key_bindings[lane_index] = None;
                    self.sequencer_key_overrides.remove(&lane_index);
                }
            }
        } else {
            for slot in &mut self.sequencer_lane_key_bindings {
                *slot = None;
            }
        }

        for lane_index in 0..lane_count {
            if self.sequencer_key_overrides.contains(&lane_index)
                && self.sequencer_lane_key_bindings.get(lane_index).copied().flatten().is_some()
            {
                continue;
            }
            let role = infer_lane_role(self.sequencer_draft.lanes.get(lane_index));
            let preferred = preferred_keys_for_lane(role, self.sequencer_keymap_mode);
            let mut assigned = None;
            for key in preferred {
                if used.insert(*key) {
                    assigned = Some(*key);
                    break;
                }
            }
            if assigned.is_none() {
                for key in SEQUENCER_ALL_PAD_KEYS {
                    if used.insert(key) {
                        assigned = Some(key);
                        break;
                    }
                }
            }
            self.sequencer_lane_key_bindings[lane_index] = assigned;
        }
    }

    pub(crate) fn current_bar_beat(&self) -> usize {
        let beats_per_bar = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4) as usize;
        (self.playhead_beats.floor() as usize) % beats_per_bar.max(1)
    }

    pub(crate) fn audio_preview(&mut self, path: &str) -> Option<&[f32]> {
        if !self.audio_preview_cache.contains_key(path) {
            let preview = build_audio_preview(path).ok()?;
            self.audio_preview_cache.insert(path.to_string(), preview);
        }
        self.audio_preview_cache.get(path).map(Vec::as_slice)
    }

    pub(crate) fn visible_bars(&self) -> u32 {
        self.project
            .as_ref()
            .map(|project| project.transport.playback_bars.max(project.transport.loop_bars).max(8))
            .unwrap_or(8)
    }

    pub(crate) fn max_timeline_beats(&self) -> f32 {
        let Some(project) = self.project.as_ref() else {
            return 8.0 * 4.0;
        };

        let beats_per_bar = project.transport.beats_per_bar as f32;
        let min_beats = self.visible_bars() as f32 * beats_per_bar;
        let loop_beats = self.loop_end_beats();
        let playhead_beats = self.playhead_beats + 1.0;

        let longest_track_beats = project
            .tracks
            .iter()
            .map(|track| {
                track.clips.iter().map(AudioClip::end_beat).fold(0.0_f32, f32::max)
            })
            .fold(0.0_f32, f32::max);

        min_beats.max(loop_beats).max(playhead_beats).max(longest_track_beats + beats_per_bar)
    }

    pub(crate) fn loop_end_beats(&self) -> f32 {
        let Some(project) = self.project.as_ref() else {
            return 0.0;
        };
        project.transport.loop_bars as f32 * project.transport.beats_per_bar as f32
    }

    pub(crate) fn selected_clip_ref(&self) -> Option<&AudioClip> {
        let selection = self.selected_clip?;
        self.project
            .as_ref()?
            .tracks
            .get(selection.track_index)?
            .clips
            .get(selection.clip_index)
    }

    pub(crate) fn step_selected_track_input(&mut self, delta: i32) -> Result<()> {
        let next = self
            .selected_track_ref()
            .and_then(|track| step_named_device(&self.input_devices, track.input_device.as_deref(), delta))
            .map(str::to_string);
        if let Some(track) = self.selected_track_mut() {
            if track.kind == TrackKind::Audio {
                track.input_device = next.clone();
                track.armed = true;
                if let Some(project) = self.project.as_mut() {
                    project.input.device = next;
                    project.dirty = true;
                }
            }
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn adjust_track_volume(&mut self, delta: f32) {
        if let Some(track) = self.selected_track_mut() {
            track.volume = (track.volume + delta).clamp(0.0, 1.0);
            self.mark_dirty();
        }
    }

    pub(crate) fn set_track_volume(&mut self, value: f32) {
        if let Some(track) = self.selected_track_mut() {
            track.volume = value.clamp(0.0, 1.0);
            self.mark_dirty();
        }
    }

    pub(crate) fn set_selected_drum_humanize(
        &mut self,
        timing_ms: f32,
        velocity_variation: f32,
        swing: f32,
        feel_ms: f32,
        evolving: bool,
    ) {
        if let Some(track) = self.selected_track_mut().filter(|track| track.kind == TrackKind::Drum) {
            track.drum_humanize.timing_ms = timing_ms.clamp(0.0, 30.0);
            track.drum_humanize.velocity_variation = velocity_variation.clamp(0.0, 0.35);
            track.drum_humanize.swing = swing.clamp(0.0, 1.0);
            track.drum_humanize.feel_ms = feel_ms.clamp(-20.0, 20.0);
            track.drum_humanize.evolving = evolving;
            self.mark_dirty();
            let _ = self.sync_router();
        }
    }

    pub(crate) fn reroll_selected_drum_humanize(&mut self) {
        if let Some(track) = self.selected_track_mut().filter(|track| track.kind == TrackKind::Drum) {
            track.drum_humanize.seed = track
                .drum_humanize
                .seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.mark_dirty();
            let _ = self.sync_router();
        }
    }

    fn build_engine_config(&self) -> Option<EngineConfig> {
        let project = self.project.as_ref()?;
        let soloed = project.tracks.iter().any(|track| track.solo);
        let sample_library = Arc::new(self.build_sample_library(project));
        let drums = project
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| track.kind == TrackKind::Drum)
            .flat_map(|(_, track)| {
                track.clips.iter().filter_map(move |clip| {
                    let mut sequence = clip.drum_sequence.clone()?;
                    sequence.humanize = track.drum_humanize.clone();
                    Some(PlaybackDrumClip {
                        active: !track.muted && (!soloed || track.solo),
                        start_beat: clip.start_beat as f64,
                        length_beats: clip.length_beats as f64,
                        source_offset_beats: clip.source_offset_beats as f64,
                        loop_count: clip.loop_count as f64,
                        volume: track.volume,
                        sequence,
                    })
                })
            })
            .collect();

        let mut clips = Vec::new();
        for track in &project.tracks {
            if track.kind != TrackKind::Audio || track.muted || (soloed && !track.solo) {
                continue;
            }
            for clip in &track.clips {
                if clip.drum_sequence.is_some() {
                    continue;
                }
                let Some(path) = clip.file_path.as_ref() else {
                    continue;
                };
                if let Some(sample) = self.cached_audio_sample(path) {
                    clips.push(PlaybackClip {
                        start_beat: clip.start_beat as f64,
                        length_beats: clip.length_beats as f64,
                        source_offset_beats: clip.source_offset_beats as f64,
                        loop_count: clip.loop_count as f64,
                        sample_rate_hz: sample.sample_rate_hz.max(1),
                        volume: track.volume,
                        samples: Arc::clone(&sample.samples),
                    });
                }
            }
        }

        Some(EngineConfig {
            route_enabled: self.route_enabled,
            input_volume: project.input.volume,
            pedals: project.pedalboard.clone(),
            playback: PlaybackState {
                playing: self.is_playing,
                playhead_beats: self.playhead_beats as f64,
                bpm: project.transport.bpm as f64,
                beats_per_bar: project.transport.beats_per_bar,
                loop_enabled: project.transport.loop_enabled,
                loop_end_beats: self.loop_end_beats() as f64,
                drum_kit: Arc::clone(&self.drum_sample_kit),
                sample_library: Arc::clone(&sample_library),
                sample_preview: Some(SamplePreview {
                    sample: self
                        .sample_preview_path
                        .as_ref()
                        .and_then(|path| sample_library.get(path))
                        .map(Arc::clone),
                    nonce: self.sample_preview_nonce,
                }),
                sequencer_preview: self.sequencer_target.as_ref().map(|target| {
                    let track_index = match target {
                        SequencerTarget::New { track_index } => *track_index,
                        SequencerTarget::Edit(selection) => selection.track_index,
                    };
                    let mut sequence = self
                        .sequencer_library_preview
                        .clone()
                        .unwrap_or_else(|| self.sequencer_draft.clone());
                    if let Some(track) = project.tracks.get(track_index) {
                        sequence.humanize = track.drum_humanize.clone();
                    }
                    SequencerPreview {
                        playing: self.sequencer_preview_playing,
                        sequence,
                        playhead_beats: self.sequencer_preview_beats as f64,
                        audition_lanes: self.sequencer_audition_lanes.clone(),
                        audition_nonce: self.sequencer_audition_nonce,
                        restart_nonce: self.sequencer_preview_restart_nonce,
                    }
                }),
                metronome: PlaybackMetronome {
                    enabled_while_playing: project.metronome.mode != MetronomeMode::Off,
                    enabled_while_idle: project.metronome.mode == MetronomeMode::Always,
                    force_tick: project.metronome.mode != MetronomeMode::Off && self.pending_record.is_some(),
                    count_in_active: self.pending_record.is_some(),
                    count_in_start_beat: self.playhead_beats as f64,
                    sound: project.metronome.sound.clone(),
                },
                drums,
                clips,
            },
            record_input: self.active_recording.is_some(),
        })
    }

    fn build_sample_library(&self, project: &Project) -> HashMap<String, Arc<AudioSample>> {
        let mut paths = HashSet::new();
        for track in &project.tracks {
            for clip in &track.clips {
                if let Some(sequence) = &clip.drum_sequence {
                    for lane in &sequence.lanes {
                        for path in lane.sample_paths() {
                            paths.insert(path.clone());
                        }
                    }
                }
            }
        }
        for lane in &self.sequencer_draft.lanes {
            for path in lane.sample_paths() {
                paths.insert(path.clone());
            }
        }
        if let Some(sequence) = &self.sequencer_library_preview {
            for lane in &sequence.lanes {
                for path in lane.sample_paths() {
                    paths.insert(path.clone());
                }
            }
        }
        if let Some(path) = &self.sample_preview_path {
            paths.insert(path.clone());
        }

        paths
            .into_iter()
            .filter_map(|path| self.cached_audio_sample(&path).map(|sample| (path, sample)))
            .collect()
    }

    fn cached_audio_sample(&self, path: &str) -> Option<Arc<AudioSample>> {
        if let Some(sample) = self.decoded_sample_cache.borrow().get(path) {
            return Some(Arc::clone(sample));
        }
        let sample = Arc::new(load_wav_samples(path).ok()?);
        self.decoded_sample_cache.borrow_mut().insert(path.to_string(), Arc::clone(&sample));
        Some(sample)
    }

    fn write_recorded_take(&self, track_index: usize, title: &str, take: &audio::RecordedTake) -> Result<String> {
        let project = self.project.as_ref().context("missing project")?;
        let base = project
            .path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.join(path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("project"))))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("projects").join("project"));
        let media_dir = base.with_extension("media");
        std::fs::create_dir_all(&media_dir)?;
        let file_name = format!("track-{}-{}.wav", track_index, slug(title));
        let path = media_dir.join(file_name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: take.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec)?;
        for sample in &take.samples {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)?;
        }
        writer.finalize()?;
        self.decoded_sample_cache.borrow_mut().remove(path.to_string_lossy().as_ref());
        Ok(path.to_string_lossy().to_string())
    }

    fn default_input_name(&self) -> Option<&str> {
        self.default_input.and_then(|index| self.input_devices.get(index)).map(String::as_str)
    }

    fn default_output_name(&self) -> Option<&str> {
        self.default_output.and_then(|index| self.output_devices.get(index)).map(String::as_str)
    }

    pub(crate) fn mark_dirty(&mut self) {
        if let Some(project) = self.project.as_mut() {
            project.dirty = true;
        }
    }

    pub(crate) fn track_name(&self, index: usize) -> String {
        self.project
            .as_ref()
            .and_then(|project| project.tracks.get(index))
            .map(|track| track.name.clone())
            .unwrap_or_default()
    }
}

fn pick_device_name<'a>(devices: &'a [String], current: Option<&str>, default: Option<usize>) -> Option<&'a str> {
    if devices.is_empty() {
        return None;
    }
    if let Some(current) = current {
        if let Some(index) = devices.iter().position(|candidate| candidate == current) {
            return devices.get(index).map(String::as_str);
        }
    }
    default.and_then(|index| devices.get(index)).or_else(|| devices.first()).map(String::as_str)
}

fn step_named_device<'a>(devices: &'a [String], current: Option<&str>, delta: i32) -> Option<&'a str> {
    if devices.is_empty() {
        return None;
    }
    let current_index = current.and_then(|name| devices.iter().position(|candidate| candidate == name)).unwrap_or(0) as i32;
    let next = (current_index + delta).rem_euclid(devices.len() as i32) as usize;
    devices.get(next).map(String::as_str)
}

fn preserve_lane_samples(previous: &DrumSequence, incoming: &mut DrumSequence) {
    for lane in &mut incoming.lanes {
        let role = lane.effective_role();
        if let Some(existing) = previous.lanes.iter().find(|candidate| candidate.effective_role() == role) {
            lane.sample_path.clone_from(&existing.sample_path);
            lane.sample_variants.clone_from(&existing.sample_variants);
            lane.gain = existing.gain;
        }
    }
}

fn append_pattern(target: &mut DrumSequence, incoming: &DrumSequence, beats_per_bar: u32) -> bool {
    if target.measures.max(1) + incoming.measures.max(1) > 8 {
        return false;
    }
    normalize_pattern_subdivision(target, SequencerSubdivision::Sixteenth, beats_per_bar);
    let mut incoming = incoming.clone();
    normalize_pattern_subdivision(&mut incoming, SequencerSubdivision::Sixteenth, beats_per_bar);
    let offset = target.total_steps(beats_per_bar);
    target.measures = target.measures.max(1) + incoming.measures.max(1);
    target.ensure_len(beats_per_bar);
    let mut matched_lanes = HashSet::new();
    for source_lane in &incoming.lanes {
        let target_index = ensure_pattern_lane(target, source_lane, beats_per_bar, &mut matched_lanes);
        if let Some(target_lane) = target.lanes.get_mut(target_index) {
            for (step, enabled) in source_lane.steps.iter().copied().enumerate() {
                let destination = offset + step;
                if destination >= target_lane.steps.len() {
                    break;
                }
                if enabled {
                    target_lane.steps[destination] = true;
                    target_lane.step_settings[destination] = source_lane.setting(step);
                }
            }
        }
    }
    true
}

fn overlay_pattern(target: &mut DrumSequence, incoming: &DrumSequence, beats_per_bar: u32) {
    normalize_pattern_subdivision(target, SequencerSubdivision::Sixteenth, beats_per_bar);
    let mut incoming = incoming.clone();
    normalize_pattern_subdivision(&mut incoming, SequencerSubdivision::Sixteenth, beats_per_bar);
    target.measures = target.measures.max(incoming.measures).clamp(1, 8);
    target.ensure_len(beats_per_bar);
    let mut matched_lanes = HashSet::new();
    for source_lane in &incoming.lanes {
        let target_index = ensure_pattern_lane(target, source_lane, beats_per_bar, &mut matched_lanes);
        if let Some(target_lane) = target.lanes.get_mut(target_index) {
            for (step, enabled) in source_lane.steps.iter().copied().enumerate() {
                if enabled && step < target_lane.steps.len() {
                    target_lane.steps[step] = true;
                    target_lane.step_settings[step] = source_lane.setting(step);
                }
            }
        }
    }
}

fn ensure_pattern_lane(
    target: &mut DrumSequence,
    source: &crate::project::DrumLane,
    beats_per_bar: u32,
    matched_lanes: &mut HashSet<usize>,
) -> usize {
    let role = source.effective_role();
    if let Some(index) = target
        .lanes
        .iter()
        .enumerate()
        .find_map(|(index, lane)| (lane.effective_role() == role && !matched_lanes.contains(&index)).then_some(index))
    {
        matched_lanes.insert(index);
        return index;
    }
    let mut lane = crate::project::DrumLane::new(&source.name, source.sample_path.clone());
    lane.role = role;
    lane.sample_variants.clone_from(&source.sample_variants);
    target.lanes.push(lane);
    target.ensure_len(beats_per_bar);
    let index = target.lanes.len() - 1;
    matched_lanes.insert(index);
    index
}

fn adapt_pattern_meter(sequence: &mut DrumSequence, source_beats_per_bar: u32, target_beats_per_bar: u32) {
    let source_beats_per_bar = source_beats_per_bar.max(1);
    let target_beats_per_bar = target_beats_per_bar.max(1);
    sequence.ensure_len(source_beats_per_bar);
    if source_beats_per_bar == target_beats_per_bar {
        return;
    }
    let total_beats = sequence.measures.max(1) * source_beats_per_bar;
    sequence.measures = total_beats.div_ceil(target_beats_per_bar).max(1);
    sequence.ensure_len(target_beats_per_bar);
}

fn normalize_pattern_subdivision(sequence: &mut DrumSequence, subdivision: SequencerSubdivision, beats_per_bar: u32) {
    if sequence.subdivision == subdivision {
        sequence.ensure_len(beats_per_bar);
        return;
    }
    let old_steps_per_beat = sequence.subdivision.steps_per_beat() as f32;
    let new_steps_per_beat = subdivision.steps_per_beat() as f32;
    let new_len = (sequence.measures.max(1) * beats_per_bar.max(1) * subdivision.steps_per_beat()) as usize;
    for lane in &mut sequence.lanes {
        let old_steps = std::mem::take(&mut lane.steps);
        let old_settings = std::mem::take(&mut lane.step_settings);
        lane.steps = vec![false; new_len];
        lane.step_settings = vec![crate::project::DrumStepSettings::default(); new_len];
        for (old_index, enabled) in old_steps.into_iter().enumerate() {
            if !enabled {
                continue;
            }
            let setting = old_settings.get(old_index).copied().unwrap_or_default();
            let beat = (old_index as f32 + setting.offset_steps) / old_steps_per_beat;
            let new_position = beat * new_steps_per_beat;
            let new_index = new_position.round().clamp(0.0, new_len.saturating_sub(1) as f32) as usize;
            if new_index < new_len {
                let mut converted = setting;
                converted.offset_steps = (new_position - new_index as f32).clamp(-0.49, 0.49);
                if !lane.steps[new_index] || converted.velocity >= lane.step_settings[new_index].velocity {
                    lane.steps[new_index] = true;
                    lane.step_settings[new_index] = converted;
                }
            }
        }
    }
    sequence.subdivision = subdivision;
}

fn load_wav_samples(path: &str) -> Result<AudioSample> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let mut out = Vec::new();

    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut sum = 0.0;
            let mut channel = 0;
            for sample in reader.samples::<f32>() {
                sum += sample?;
                channel += 1;
                if channel == channels {
                    out.push((sum / channels as f32).clamp(-1.0, 1.0));
                    sum = 0.0;
                    channel = 0;
                }
            }
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample.clamp(1, 32);
            let scale = ((1_i64 << (bits - 1)) - 1).max(1) as f32;
            let mut sum = 0.0;
            let mut channel = 0;
            for sample in reader.samples::<i32>() {
                sum += sample? as f32 / scale;
                channel += 1;
                if channel == channels {
                    out.push((sum / channels as f32).clamp(-1.0, 1.0));
                    sum = 0.0;
                    channel = 0;
                }
            }
        }
    }

    Ok(AudioSample {
        samples: Arc::from(out),
        sample_rate_hz: spec.sample_rate,
    })
}

fn silent_audio_sample() -> AudioSample {
    AudioSample {
        samples: Arc::from(vec![0.0_f32]),
        sample_rate_hz: 44_100,
    }
}

fn build_audio_preview(path: &str) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let total_frames = (reader.duration() as usize / channels.max(1)).max(1);
    let bin_count = 256usize.min(total_frames.max(1));
    let mut bins = vec![0.0_f32; bin_count];
    let mut counts = vec![0usize; bin_count];
    let scale = if spec.sample_format == hound::SampleFormat::Float {
        None
    } else if spec.bits_per_sample <= 16 {
        Some(i16::MAX as f32)
    } else {
        Some(i32::MAX as f32)
    };

    let mut frame_index = 0usize;
    let mut channel_index = 0usize;
    match scale {
        None => {
            for sample in reader.samples::<f32>().flatten() {
                if channel_index == 0 {
                    let bucket = ((frame_index * bin_count) / total_frames).min(bin_count - 1);
                    let value = sample.clamp(-1.0, 1.0);
                    bins[bucket] += value * value;
                    counts[bucket] += 1;
                    frame_index += 1;
                }
                channel_index = (channel_index + 1) % channels;
            }
        }
        Some(divisor) => {
            for sample in reader.samples::<i32>().flatten() {
                if channel_index == 0 {
                    let normalized = (sample as f32 / divisor).clamp(-1.0, 1.0);
                    let bucket = ((frame_index * bin_count) / total_frames).min(bin_count - 1);
                    bins[bucket] += normalized * normalized;
                    counts[bucket] += 1;
                    frame_index += 1;
                }
                channel_index = (channel_index + 1) % channels;
            }
        }
    }

    let mut max_value = 0.0_f32;
    for (bin, count) in bins.iter_mut().zip(counts.iter()) {
        if *count > 0 {
            *bin = (*bin / *count as f32).sqrt();
            max_value = max_value.max(*bin);
        }
    }
    if max_value > 0.0 {
        for bin in &mut bins {
            *bin /= max_value;
        }
    }
    Ok(bins)
}

fn load_default_drum_sample_kit(paths: &ContentPaths) -> DrumSampleKit {
    let base = paths.kits.join("salamander/OH");
    DrumSampleKit {
        kick: load_content_layers(&base, "kick_OH_", &[("P_", &[1, 3, 5]), ("F_", &[1, 5, 9]), ("FF_", &[1, 5, 9])]),
        snare: load_content_layers(
            &base,
            "snare_OH_",
            &[("Ghost_", &[1, 4, 7]), ("MP_", &[1, 5, 9]), ("F_", &[1, 5, 9]), ("FF_", &[1, 4, 7])],
        ),
        closed_hat: load_content_layers(&base, "hihatClosed_OH_", &[("P_", &[1, 8, 16]), ("F_", &[1, 8, 16])]),
        open_hat: vec![load_content_samples(&base, "hihatOpen_OH_F_", &[1, 3, 5])],
        pedal_hat: vec![load_content_samples(&base, "hihatFoot_OH_MP_", &[1, 5, 10])],
        high_tom: load_content_layers(&base, "hiTom_OH_", &[("P_", &[1, 5, 9]), ("F_", &[1, 5, 9]), ("FF_", &[1, 5, 9])]),
        low_tom: vec![load_content_samples(&base, "loTom_OH_MP_", &[1, 5, 10])],
        ride: load_content_layers(&base, "ride2_OH_", &[("PP_", &[1, 2, 4]), ("MP_", &[1, 4, 8]), ("FF_", &[1, 3, 5])]),
        crash: vec![load_content_samples(&base, "crash2_OH_FF_", &[2, 4, 7])],
        percussion: vec![load_content_samples(&base, "snareStick_OH_F_", &[1, 5, 9])],
    }
}

fn load_content_layers(
    base: &std::path::Path,
    prefix: &str,
    layers: &[(&str, &[u8])],
) -> Vec<Vec<Arc<AudioSample>>> {
    layers
        .iter()
        .map(|(dynamic, recordings)| load_content_samples(base, &format!("{prefix}{dynamic}"), recordings))
        .collect()
}

fn load_content_samples(base: &std::path::Path, prefix: &str, layers: &[u8]) -> Vec<Arc<AudioSample>> {
    layers.iter().map(|layer| load_content_sample(base.join(format!("{prefix}{layer}.wav")))).collect()
}

fn load_content_sample(path: PathBuf) -> Arc<AudioSample> {
    load_wav_samples(path.to_string_lossy().as_ref())
        .map(Arc::new)
        .unwrap_or_else(|_| Arc::new(silent_audio_sample()))
}

const SEQUENCER_ALL_PAD_KEYS: [SequencerPadKey; 26] = [
    SequencerPadKey::Q,
    SequencerPadKey::W,
    SequencerPadKey::E,
    SequencerPadKey::R,
    SequencerPadKey::T,
    SequencerPadKey::Y,
    SequencerPadKey::U,
    SequencerPadKey::I,
    SequencerPadKey::O,
    SequencerPadKey::P,
    SequencerPadKey::A,
    SequencerPadKey::S,
    SequencerPadKey::D,
    SequencerPadKey::F,
    SequencerPadKey::G,
    SequencerPadKey::H,
    SequencerPadKey::J,
    SequencerPadKey::K,
    SequencerPadKey::L,
    SequencerPadKey::Z,
    SequencerPadKey::X,
    SequencerPadKey::C,
    SequencerPadKey::V,
    SequencerPadKey::B,
    SequencerPadKey::N,
    SequencerPadKey::M,
];

const ASDF_KEYS: [SequencerPadKey; 26] = [
    SequencerPadKey::A,
    SequencerPadKey::S,
    SequencerPadKey::D,
    SequencerPadKey::F,
    SequencerPadKey::G,
    SequencerPadKey::H,
    SequencerPadKey::J,
    SequencerPadKey::K,
    SequencerPadKey::L,
    SequencerPadKey::Q,
    SequencerPadKey::W,
    SequencerPadKey::E,
    SequencerPadKey::R,
    SequencerPadKey::T,
    SequencerPadKey::Y,
    SequencerPadKey::U,
    SequencerPadKey::I,
    SequencerPadKey::O,
    SequencerPadKey::P,
    SequencerPadKey::Z,
    SequencerPadKey::X,
    SequencerPadKey::C,
    SequencerPadKey::V,
    SequencerPadKey::B,
    SequencerPadKey::N,
    SequencerPadKey::M,
];

const DRUMKIT_KICK_KEYS: [SequencerPadKey; 4] = [
    SequencerPadKey::B,
    SequencerPadKey::V,
    SequencerPadKey::N,
    SequencerPadKey::G,
];
const DRUMKIT_SNARE_KEYS: [SequencerPadKey; 4] = [
    SequencerPadKey::C,
    SequencerPadKey::X,
    SequencerPadKey::D,
    SequencerPadKey::F,
];
const DRUMKIT_CYMBAL_KEYS: [SequencerPadKey; 10] = [
    SequencerPadKey::Q,
    SequencerPadKey::W,
    SequencerPadKey::E,
    SequencerPadKey::R,
    SequencerPadKey::T,
    SequencerPadKey::Y,
    SequencerPadKey::U,
    SequencerPadKey::I,
    SequencerPadKey::O,
    SequencerPadKey::P,
];
const DRUMKIT_TOM_KEYS: [SequencerPadKey; 6] = [
    SequencerPadKey::H,
    SequencerPadKey::J,
    SequencerPadKey::K,
    SequencerPadKey::L,
    SequencerPadKey::N,
    SequencerPadKey::M,
];
const DRUMKIT_PERC_KEYS: [SequencerPadKey; 11] = [
    SequencerPadKey::A,
    SequencerPadKey::S,
    SequencerPadKey::D,
    SequencerPadKey::F,
    SequencerPadKey::G,
    SequencerPadKey::Z,
    SequencerPadKey::X,
    SequencerPadKey::C,
    SequencerPadKey::V,
    SequencerPadKey::M,
    SequencerPadKey::L,
];

fn preferred_keys_for_lane(role: SequencerLaneRole, mode: SequencerKeymapMode) -> &'static [SequencerPadKey] {
    match mode {
        SequencerKeymapMode::Asdf => &ASDF_KEYS,
        SequencerKeymapMode::DrumKit | SequencerKeymapMode::Custom => match role {
            SequencerLaneRole::Kick => &DRUMKIT_KICK_KEYS,
            SequencerLaneRole::Snare => &DRUMKIT_SNARE_KEYS,
            SequencerLaneRole::Cymbal => &DRUMKIT_CYMBAL_KEYS,
            SequencerLaneRole::Tom => &DRUMKIT_TOM_KEYS,
            SequencerLaneRole::Perc => &DRUMKIT_PERC_KEYS,
        },
    }
}

fn infer_lane_role(lane: Option<&crate::project::DrumLane>) -> SequencerLaneRole {
    let Some(lane) = lane else {
        return SequencerLaneRole::Perc;
    };
    let mut text = lane.name.to_ascii_lowercase();
    if let Some(path) = lane.sample_path.as_ref() {
        text.push(' ');
        text.push_str(path);
    }
    if text.contains("kick") || text.contains("808") || text.contains("bass") {
        return SequencerLaneRole::Kick;
    }
    if text.contains("snare") || text.contains("rim") || text.contains("clap") {
        return SequencerLaneRole::Snare;
    }
    if text.contains("hat")
        || text.contains("cymbal")
        || text.contains("crash")
        || text.contains("ride")
        || text.contains("shaker")
    {
        return SequencerLaneRole::Cymbal;
    }
    if text.contains("tom") {
        return SequencerLaneRole::Tom;
    }
    SequencerLaneRole::Perc
}

fn sample_filter_for_path(path: &str) -> SampleBrowserFilter {
    let text = path.to_ascii_lowercase();
    if text.contains("kick") || text.contains("808") || text.contains("bass") {
        return SampleBrowserFilter::Kick;
    }
    if text.contains("snare") || text.contains("clap") || text.contains("rim") {
        return SampleBrowserFilter::Snare;
    }
    if text.contains("hat") {
        return SampleBrowserFilter::Hat;
    }
    if text.contains("tom") {
        return SampleBrowserFilter::Tom;
    }
    if text.contains("cymbal") || text.contains("crash") || text.contains("ride") {
        return SampleBrowserFilter::Cymbal;
    }
    if text.contains("fx") || text.contains("sweep") || text.contains("impact") {
        return SampleBrowserFilter::Fx;
    }
    SampleBrowserFilter::Perc
}

fn collect_sample_browser_entries(root: &PathBuf, dir: &PathBuf, out: &mut Vec<SampleBrowserEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sample_browser_entries(root, &path, out);
            continue;
        }
        if !is_audio_sample_path(&path) {
            continue;
        }
        let title = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("sample")
            .to_string();
        let folder = path
            .strip_prefix(root)
            .ok()
            .and_then(|relative| relative.parent())
            .map(|parent| parent.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let path_str = path.to_string_lossy().to_string();
        out.push(SampleBrowserEntry {
            category: sample_filter_for_path(&path_str),
            path: path_str,
            title,
            folder,
        });
    }
}

fn is_audio_sample_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "wav"
    )
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub(crate) fn short_path(path: &std::path::Path) -> String {
    path.file_name().and_then(|name| name.to_str()).unwrap_or("project").to_string()
}

pub(crate) fn track_action_label(track: &Track) -> &'static str {
    match track.kind {
        TrackKind::Drum => "SEQ",
        TrackKind::Audio if track.overwrite => "OVR",
        TrackKind::Audio => "REC",
    }
}

pub(crate) fn sequence_step_count(sequence: &DrumSequence, beats_per_bar: u32) -> usize {
    sequence.total_steps(beats_per_bar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdivision_conversion_preserves_absolute_hit_time() {
        let mut sequence = DrumSequence::default();
        sequence.measures = 1;
        sequence.subdivision = SequencerSubdivision::Quarter;
        let mut lane = crate::project::DrumLane::new("Snare", None);
        lane.steps = vec![true, false, false, false];
        lane.step_settings = vec![crate::project::DrumStepSettings::default(); 4];
        lane.step_settings[0].offset_steps = 0.25;
        sequence.lanes.push(lane);
        normalize_pattern_subdivision(&mut sequence, SequencerSubdivision::Sixteenth, 4);
        assert!(sequence.lanes[0].steps[1]);
        assert!(sequence.lanes[0].step_settings[1].offset_steps.abs() < 0.001);
    }

    #[test]
    fn append_rejects_patterns_over_the_sequence_limit() {
        let mut target = DrumSequence::default();
        target.measures = 8;
        target.ensure_len(4);
        let incoming = DrumSequence::default();
        assert!(!append_pattern(&mut target, &incoming, 4));
        assert_eq!(target.measures, 8);
    }

    #[test]
    fn meter_adaptation_preserves_hit_beat_positions() {
        let mut sequence = DrumSequence::default();
        sequence.measures = 2;
        let mut lane = crate::project::DrumLane::new("Kick", None);
        lane.steps = vec![false; 24];
        lane.step_settings = vec![crate::project::DrumStepSettings::default(); 24];
        lane.steps[20] = true;
        sequence.lanes.push(lane);
        adapt_pattern_meter(&mut sequence, 3, 4);
        assert_eq!(sequence.measures, 2);
        assert!(sequence.lanes[0].steps[20]);
    }

    #[test]
    fn overlay_keeps_multiple_lanes_with_the_same_role() {
        let mut target = DrumSequence::default();
        target.lanes.clear();
        let mut incoming = DrumSequence::default();
        incoming.lanes = vec![
            crate::project::DrumLane::new("Snare A", None),
            crate::project::DrumLane::new("Snare B", None),
        ];
        for lane in &mut incoming.lanes {
            lane.role = crate::project::DrumRole::Snare;
        }
        incoming.ensure_len(4);
        overlay_pattern(&mut target, &incoming, 4);
        assert_eq!(target.lanes.len(), 2);
    }
}
