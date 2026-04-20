use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::audio::{self, EngineConfig, EngineHandle, PlaybackClip, PlaybackDrumClip, PlaybackMetronome, PlaybackState};
use crate::pedals::{PedalKind, PedalSpec};
use crate::project::{self, AudioClip, DrumSequence, MetronomeMode, Project, ProjectSummary, Track, TrackKind};

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
}

impl App {
    pub fn new() -> Result<Self> {
        let inventory = audio::discover_devices()?;
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
        })
    }

    pub fn refresh(&mut self) {
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

    fn rebuild_engine(&mut self) -> Result<()> {
        self.stop_router()?;
        self.sync_router()
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

    pub(crate) fn step_output_device(&mut self, delta: i32) -> Result<()> {
        let next = self
            .project
            .as_ref()
            .and_then(|project| step_named_device(&self.output_devices, project.output_device.as_deref(), delta))
            .map(str::to_string);
        if let Some(project) = self.project.as_mut() {
            project.output_device = next;
            project.dirty = true;
        }
        self.sync_router()?;
        Ok(())
    }

    pub(crate) fn step_input_device(&mut self, delta: i32) -> Result<()> {
        let next = self
            .project
            .as_ref()
            .and_then(|project| step_named_device(&self.input_devices, project.input.device.as_deref(), delta))
            .map(str::to_string);
        if let Some(project) = self.project.as_mut() {
            project.input.device = next.clone();
            for track in &mut project.tracks {
                if track.kind == TrackKind::Audio && track.armed {
                    track.input_device = next.clone();
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
        self.rebuild_engine()?;
        Ok(())
    }

    pub(crate) fn set_input_volume(&mut self, value: f32) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            project.input.volume = value.clamp(0.0, 1.0);
            project.dirty = true;
        }
        self.rebuild_engine()?;
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
        let _ = self.sync_router();
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
        self.rebuild_engine()?;
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
        self.rebuild_engine()?;
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
        self.rebuild_engine()?;
        Ok(())
    }

    pub(crate) fn toggle_pedal_enabled(&mut self, index: usize) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            if let Some(pedal) = project.pedalboard.get_mut(index) {
                pedal.toggle_enabled();
                project.dirty = true;
            }
        }
        self.rebuild_engine()?;
        Ok(())
    }

    pub(crate) fn adjust_pedal_param(&mut self, index: usize, param_index: usize, delta: i32) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            if let Some(pedal) = project.pedalboard.get_mut(index) {
                pedal.step_param(param_index, delta);
                project.dirty = true;
            }
        }
        self.rebuild_engine()?;
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
        self.sequencer_target = Some(SequencerTarget::New {
            track_index: self.selected_track,
        });
        self.sequencer_popup_open = true;
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
        self.sequencer_target = Some(SequencerTarget::Edit(selection));
        self.sequencer_popup_open = true;
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
        self.mark_dirty();
        let _ = self.sync_router();
    }

    pub(crate) fn cancel_sequence_chunk(&mut self) {
        self.sequencer_popup_open = false;
        self.sequencer_target = None;
    }

    pub(crate) fn adjust_selected_sequence_measures(&mut self, delta: i32) {
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.measures = (self.sequencer_draft.measures as i32 + delta).clamp(1, 8) as u32;
        self.sequencer_draft.ensure_len(beats);
    }

    pub(crate) fn adjust_selected_sequence_subdivision(&mut self, delta: i32) {
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.subdivision = self.sequencer_draft.subdivision.step(delta);
        self.sequencer_draft.ensure_len(beats);
    }

    pub(crate) fn toggle_sequence_step(&mut self, lane_index: usize, step_index: usize) {
        let beats = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4);
        self.sequencer_draft.ensure_len(beats);
        if let Some(step) = self.sequencer_draft.lanes.get_mut(lane_index).and_then(|lane| lane.steps.get_mut(step_index)) {
            *step = !*step;
        }
    }

    pub(crate) fn sequence(&self) -> Option<&DrumSequence> {
        self.sequencer_target.as_ref().map(|_| &self.sequencer_draft)
    }

    pub(crate) fn current_bar_beat(&self) -> usize {
        let beats_per_bar = self.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4) as usize;
        (self.playhead_beats.floor() as usize) % beats_per_bar.max(1)
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
                let clip_end = track
                    .clips
                    .iter()
                    .map(AudioClip::end_beat)
                    .fold(0.0_f32, f32::max);
                let sequence_end = if track.kind == TrackKind::Drum {
                    track.sequencer.measures.max(1) as f32 * beats_per_bar
                } else {
                    0.0
                };
                clip_end.max(sequence_end)
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

    fn build_engine_config(&self) -> Option<EngineConfig> {
        let project = self.project.as_ref()?;
        let soloed = project.tracks.iter().any(|track| track.solo);
        let drums = project
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| track.kind == TrackKind::Drum)
            .flat_map(|(_, track)| {
                track.clips.iter().filter_map(move |clip| {
                    Some(PlaybackDrumClip {
                        active: !track.muted && (!soloed || track.solo),
                        start_beat: clip.start_beat as f64,
                        length_beats: clip.length_beats as f64,
                        source_offset_beats: clip.source_offset_beats as f64,
                        loop_count: clip.loop_count as f64,
                        volume: track.volume,
                        sequence: clip.drum_sequence.clone()?,
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
                let Some(path) = clip.file_path.as_ref() else {
                    continue;
                };
                if clip.drum_sequence.is_none() {
                if let Ok(samples) = load_wav_samples(path) {
                    clips.push(PlaybackClip {
                        start_beat: clip.start_beat as f64,
                        length_beats: clip.length_beats as f64,
                        source_offset_beats: clip.source_offset_beats as f64,
                        loop_count: clip.loop_count as f64,
                        volume: track.volume,
                        samples: Arc::from(samples),
                    });
                }
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

fn load_wav_samples(path: &str) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let mut out = Vec::new();

    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut frame = Vec::with_capacity(channels);
            for sample in reader.samples::<f32>() {
                frame.push(sample?);
                if frame.len() == channels {
                    out.push(frame[0].clamp(-1.0, 1.0));
                    frame.clear();
                }
            }
        }
        hound::SampleFormat::Int => {
            let scale = if spec.bits_per_sample <= 16 {
                i16::MAX as f32
            } else {
                i32::MAX as f32
            };
            let mut frame = Vec::with_capacity(channels);
            for sample in reader.samples::<i32>() {
                frame.push(sample? as f32 / scale);
                if frame.len() == channels {
                    out.push(frame[0].clamp(-1.0, 1.0));
                    frame.clear();
                }
            }
        }
    }

    Ok(out)
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
