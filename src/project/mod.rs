use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::pedals::{MetronomePedal, PedalSpec};

const PROJECTS_DIR: &str = "projects";
const PROJECT_EXT: &str = "lupe.db";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub transport: Transport,
    pub output_device: Option<String>,
    #[serde(default)]
    pub input: InputSettings,
    #[serde(default)]
    pub metronome: MetronomeSettings,
    #[serde(default)]
    pub pedalboard: Vec<PedalSpec>,
    pub tracks: Vec<Track>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
    #[serde(skip)]
    pub dirty: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transport {
    pub bpm: u32,
    pub beats_per_bar: u32,
    pub beat_unit: u32,
    pub loop_bars: u32,
    pub loop_enabled: bool,
    #[serde(default)]
    pub playback_bars: u32,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            bpm: 110,
            beats_per_bar: 4,
            beat_unit: 4,
            loop_bars: 4,
            loop_enabled: true,
            playback_bars: 8,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputSettings {
    pub device: Option<String>,
    pub volume: f32,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            device: None,
            volume: 0.85,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetronomeMode {
    Off,
    On,
    Always,
}

impl MetronomeMode {
    pub const ALL: [MetronomeMode; 3] = [MetronomeMode::Off, MetronomeMode::On, MetronomeMode::Always];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
            Self::Always => "Always",
        }
    }

    pub fn step(self, delta: i32) -> Self {
        let current = Self::ALL.iter().position(|candidate| *candidate == self).unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(Self::ALL.len() as i32) as usize;
        Self::ALL[next]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetronomeSettings {
    pub mode: MetronomeMode,
    pub sound: MetronomePedal,
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self {
            mode: MetronomeMode::On,
            sound: MetronomePedal::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Drum,
    Audio,
}

impl TrackKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Drum => "drum",
            Self::Audio => "audio",
        }
    }

    fn db_value(self) -> &'static str {
        self.label()
    }

    fn from_db(value: &str) -> Self {
        match value {
            "drum" => Self::Drum,
            _ => Self::Audio,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub kind: TrackKind,
    pub color: TrackColor,
    pub muted: bool,
    pub solo: bool,
    pub armed: bool,
    pub overwrite: bool,
    pub volume: f32,
    pub input_device: Option<String>,
    #[serde(default)]
    pub clips: Vec<AudioClip>,
    #[serde(default)]
    pub count_in_enabled: bool,
    #[serde(default = "default_count_in_beats")]
    pub count_in_beats: u32,
    #[serde(default)]
    pub sequencer: DrumSequence,
    #[serde(default)]
    pub pedals: Vec<PedalSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioClip {
    pub start_beat: f32,
    pub length_beats: f32,
    pub source_track: usize,
    pub title: String,
    #[serde(default)]
    pub source_offset_beats: f32,
    #[serde(default = "default_loop_count")]
    pub loop_count: f32,
    #[serde(default)]
    pub file_path: Option<String>,
}

impl AudioClip {
    pub fn span_beats(&self) -> f32 {
        (self.length_beats.max(0.25) * self.loop_count.max(0.25)).max(0.25)
    }

    pub fn end_beat(&self) -> f32 {
        self.start_beat + self.span_beats()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumSequence {
    pub measures: u32,
    pub subdivision: SequencerSubdivision,
    pub lanes: Vec<DrumLane>,
}

impl Default for DrumSequence {
    fn default() -> Self {
        Self {
            measures: 2,
            subdivision: SequencerSubdivision::Quarter,
            lanes: vec![
                DrumLane::new("Kick"),
                DrumLane::new("Snare"),
                DrumLane::new("Closed Hat"),
                DrumLane::new("Open Hat"),
                DrumLane::new("Clap"),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SequencerSubdivision {
    Quarter,
    Eighth,
    Triplet,
    Sixteenth,
}

impl SequencerSubdivision {
    pub const ALL: [SequencerSubdivision; 4] = [
        SequencerSubdivision::Quarter,
        SequencerSubdivision::Eighth,
        SequencerSubdivision::Triplet,
        SequencerSubdivision::Sixteenth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Quarter => "Beat",
            Self::Eighth => "Half beat",
            Self::Triplet => "Third beat",
            Self::Sixteenth => "Quarter beat",
        }
    }

    pub fn steps_per_beat(self) -> u32 {
        match self {
            Self::Quarter => 1,
            Self::Eighth => 2,
            Self::Triplet => 3,
            Self::Sixteenth => 4,
        }
    }

    pub fn step(self, delta: i32) -> Self {
        let current = Self::ALL.iter().position(|candidate| *candidate == self).unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(Self::ALL.len() as i32) as usize;
        Self::ALL[next]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumLane {
    pub name: String,
    #[serde(default)]
    pub steps: Vec<bool>,
}

impl DrumLane {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackColor {
    Blue,
    Green,
    Yellow,
    Magenta,
    Cyan,
    Red,
}

impl TrackColor {
    pub const ALL: [TrackColor; 6] = [
        TrackColor::Blue,
        TrackColor::Green,
        TrackColor::Yellow,
        TrackColor::Magenta,
        TrackColor::Cyan,
        TrackColor::Red,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Magenta => "magenta",
            Self::Cyan => "cyan",
            Self::Red => "red",
        }
    }

    fn from_db(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|color| color.label() == value)
            .unwrap_or(Self::Blue)
    }

    pub fn step(self, delta: i32) -> Self {
        let current = Self::ALL.iter().position(|candidate| *candidate == self).unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(Self::ALL.len() as i32) as usize;
        Self::ALL[next]
    }
}

#[derive(Clone, Debug)]
pub struct ProjectSummary {
    pub name: String,
    pub path: PathBuf,
    pub modified: SystemTime,
}

impl Project {
    pub fn new_default(name: String, default_input: Option<&str>, default_output: Option<&str>) -> Self {
        let mut project = Self {
            name,
            transport: Transport::default(),
            output_device: default_output.map(str::to_string),
            input: InputSettings {
                device: default_input.map(str::to_string),
                volume: 0.85,
            },
            metronome: MetronomeSettings::default(),
            pedalboard: Vec::new(),
            tracks: vec![Track::new_drum(), Track::new_audio(1, default_input)],
            path: None,
            dirty: true,
        };
        project.ensure_invariants();
        project
    }

    pub fn ensure_invariants(&mut self) {
        if self.tracks.is_empty() {
            self.tracks.push(Track::new_drum());
        }
        if self.tracks.first().map(|track| track.kind) != Some(TrackKind::Drum) {
            self.tracks.insert(0, Track::new_drum());
        }
        if self.tracks.len() == 1 {
            let input = self.input.device.clone();
            self.tracks.push(Track::new_audio(1, input.as_deref()));
        }
        for (index, track) in self.tracks.iter_mut().enumerate() {
            track.volume = track.volume.clamp(0.0, 1.0);
            track.count_in_beats = track.count_in_beats.clamp(1, 8);
            if track.kind == TrackKind::Drum {
                track.input_device = None;
                track.sequencer.ensure_len(self.transport.beats_per_bar);
            }
            for clip in &mut track.clips {
                clip.source_track = index;
                clip.length_beats = clip.length_beats.max(0.25);
            }
        }
        self.transport.bpm = self.transport.bpm.clamp(40, 240);
        self.transport.beats_per_bar = self.transport.beats_per_bar.clamp(1, 12);
        self.transport.beat_unit = self.transport.beat_unit.clamp(1, 16);
        self.transport.loop_bars = self.transport.loop_bars.clamp(1, 32);
        self.transport.playback_bars = self.transport.playback_bars.max(self.transport.loop_bars).clamp(4, 64);
        self.input.volume = self.input.volume.clamp(0.0, 1.0);
    }
}

impl Track {
    pub fn new_drum() -> Self {
        Self {
            name: "Drums".to_string(),
            kind: TrackKind::Drum,
            color: TrackColor::Yellow,
            muted: false,
            solo: false,
            armed: false,
            overwrite: false,
            volume: 0.85,
            input_device: None,
            clips: Vec::new(),
            count_in_enabled: false,
            count_in_beats: default_count_in_beats(),
            sequencer: DrumSequence::default(),
            pedals: Vec::new(),
        }
    }

    pub fn new_audio(number: usize, default_input: Option<&str>) -> Self {
        Self {
            name: if number <= 1 {
                "Recording".to_string()
            } else {
                format!("Recording {number}")
            },
            kind: TrackKind::Audio,
            color: TrackColor::Cyan,
            muted: false,
            solo: false,
            armed: number == 1,
            overwrite: false,
            volume: 0.85,
            input_device: default_input.map(str::to_string),
            clips: Vec::new(),
            count_in_enabled: true,
            count_in_beats: default_count_in_beats(),
            sequencer: DrumSequence::default(),
            pedals: Vec::new(),
        }
    }
}

impl DrumSequence {
    pub fn total_steps(&self, beats_per_bar: u32) -> usize {
        (self.measures.max(1) * beats_per_bar.max(1) * self.subdivision.steps_per_beat()) as usize
    }

    pub fn ensure_len(&mut self, beats_per_bar: u32) {
        let total_steps = self.total_steps(beats_per_bar);
        for lane in &mut self.lanes {
            lane.steps.resize(total_steps, false);
        }
    }
}

pub fn projects_dir() -> Result<PathBuf> {
    let path = std::env::current_dir()?.join(PROJECTS_DIR);
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn list_recent_projects() -> Result<Vec<ProjectSummary>> {
    let mut projects = Vec::new();
    for entry in fs::read_dir(projects_dir()?)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("db") {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let name = load_project_name(&path).unwrap_or_else(|_| display_name_from_path(&path));
        projects.push(ProjectSummary { name, path, modified });
    }

    projects.sort_by(|left, right| right.modified.cmp(&left.modified));
    Ok(projects)
}

pub fn save_project(project: &mut Project) -> Result<()> {
    let path = project
        .path
        .clone()
        .unwrap_or_else(|| projects_dir().unwrap_or_else(|_| PathBuf::from(PROJECTS_DIR)).join(default_file_name(&project.name)));
    save_project_to(project, &path)?;
    project.path = Some(path);
    project.dirty = false;
    Ok(())
}

pub fn project_name_available(current_path: Option<&Path>, name: &str) -> Result<bool> {
    let candidate = projects_dir()?.join(default_file_name(name));
    Ok(!candidate.exists() || current_path == Some(candidate.as_path()))
}

pub fn rename_project(project: &mut Project, new_name: &str) -> Result<()> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        anyhow::bail!("project name cannot be empty");
    }

    let new_path = projects_dir()?.join(default_file_name(new_name));
    if !project_name_available(project.path.as_deref(), new_name)? {
        anyhow::bail!("project name is already in use");
    }

    if let Some(old_path) = project.path.as_ref() {
        if old_path != &new_path && old_path.exists() {
            fs::rename(old_path, &new_path)?;
        }
    }

    project.name = new_name.to_string();
    project.path = Some(new_path);
    project.dirty = true;
    save_project(project)
}

pub fn save_project_to(project: &Project, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tracks (
            position INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            name TEXT NOT NULL,
            color TEXT NOT NULL,
            muted INTEGER NOT NULL,
            solo INTEGER NOT NULL,
            armed INTEGER NOT NULL,
            overwrite_mode INTEGER NOT NULL,
            volume REAL NOT NULL,
            input_device TEXT
        );
        CREATE TABLE IF NOT EXISTS pedals (
            track_position INTEGER NOT NULL,
            position INTEGER NOT NULL,
            kind TEXT NOT NULL,
            data_json TEXT NOT NULL,
            PRIMARY KEY (track_position, position)
        );
        ",
    )?;

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM meta", [])?;
    tx.execute("DELETE FROM tracks", [])?;
    tx.execute("DELETE FROM pedals", [])?;

    insert_meta(&tx, "name", &project.name)?;
    insert_meta(&tx, "project_json", &serde_json::to_string(project)?)?;
    insert_meta(&tx, "bpm", &project.transport.bpm.to_string())?;
    insert_meta(&tx, "beats_per_bar", &project.transport.beats_per_bar.to_string())?;
    insert_meta(&tx, "loop_bars", &project.transport.loop_bars.to_string())?;
    insert_meta(&tx, "metronome_enabled", if project.metronome.mode == MetronomeMode::Off { "0" } else { "1" })?;
    insert_meta(
        &tx,
        "count_in_enabled",
        if project.tracks.iter().any(|track| track.count_in_enabled) { "1" } else { "0" },
    )?;
    insert_meta(&tx, "output_device", project.output_device.as_deref().unwrap_or(""))?;

    for (position, track) in project.tracks.iter().enumerate() {
        tx.execute(
            "INSERT INTO tracks (position, kind, name, color, muted, solo, armed, overwrite_mode, volume, input_device)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                position as i64,
                track.kind.db_value(),
                track.name,
                track.color.label(),
                bool_to_i64(track.muted),
                bool_to_i64(track.solo),
                bool_to_i64(track.armed),
                bool_to_i64(track.overwrite),
                track.volume,
                track.input_device,
            ],
        )?;

        for (pedal_position, pedal) in track.pedals.iter().enumerate() {
            tx.execute(
                "INSERT INTO pedals (track_position, position, kind, data_json) VALUES (?1, ?2, ?3, ?4)",
                params![
                    position as i64,
                    pedal_position as i64,
                    pedal.label(),
                    serde_json::to_string(pedal)?,
                ],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn load_project(path: &Path) -> Result<Project> {
    let conn = Connection::open(path)?;

    if let Some(project_json) = load_meta(&conn, "project_json")? {
        let mut project: Project = serde_json::from_str(&project_json).context("failed to decode project json")?;
        project.path = Some(path.to_path_buf());
        project.dirty = false;
        project.ensure_invariants();
        return Ok(project);
    }

    let name = load_meta(&conn, "name")?.unwrap_or_else(|| display_name_from_path(path));
    let bpm = load_meta(&conn, "bpm")?.and_then(|value| value.parse().ok()).unwrap_or(110);
    let beats_per_bar = load_meta(&conn, "beats_per_bar")?.and_then(|value| value.parse().ok()).unwrap_or(4);
    let loop_bars = load_meta(&conn, "loop_bars")?.and_then(|value| value.parse().ok()).unwrap_or(4);
    let metronome_enabled = load_meta(&conn, "metronome_enabled")?.as_deref() != Some("0");
    let count_in_enabled = load_meta(&conn, "count_in_enabled")?.as_deref() != Some("0");
    let output_device = load_meta(&conn, "output_device")?.filter(|value| !value.is_empty());

    let mut statement = conn.prepare(
        "SELECT position, kind, name, color, muted, solo, armed, overwrite_mode, volume, input_device
         FROM tracks ORDER BY position ASC",
    )?;
    let mut rows = statement.query([])?;
    let mut tracks = Vec::new();
    while let Some(row) = rows.next()? {
        let position: i64 = row.get(0)?;
        let kind: String = row.get(1)?;
        let name: String = row.get(2)?;
        let color: String = row.get(3)?;
        let muted: i64 = row.get(4)?;
        let solo: i64 = row.get(5)?;
        let armed: i64 = row.get(6)?;
        let overwrite: i64 = row.get(7)?;
        let volume: f32 = row.get(8)?;
        let input_device: Option<String> = row.get(9)?;

        let pedals = load_track_pedals(&conn, position)?;
        tracks.push(Track {
            name,
            kind: TrackKind::from_db(&kind),
            color: TrackColor::from_db(&color),
            muted: muted != 0,
            solo: solo != 0,
            armed: armed != 0,
            overwrite: overwrite != 0,
            volume,
            input_device,
            clips: Vec::new(),
            count_in_enabled,
            count_in_beats: default_count_in_beats(),
            sequencer: DrumSequence::default(),
            pedals,
        });
    }

    let mut project = Project {
        name,
        transport: Transport {
            bpm,
            beats_per_bar,
            beat_unit: 4,
            loop_bars,
            loop_enabled: true,
            playback_bars: loop_bars.max(8),
        },
        output_device,
        input: InputSettings::default(),
        metronome: MetronomeSettings {
            mode: if metronome_enabled { MetronomeMode::On } else { MetronomeMode::Off },
            sound: MetronomePedal::default(),
        },
        pedalboard: Vec::new(),
        tracks,
        path: Some(path.to_path_buf()),
        dirty: false,
    };

    if let Some(first_audio) = project.tracks.iter().find(|track| track.kind == TrackKind::Audio) {
        project.input.device = first_audio.input_device.clone();
    }

    project.ensure_invariants();
    Ok(project)
}

fn default_count_in_beats() -> u32 {
    4
}

fn default_loop_count() -> f32 {
    1.0
}

fn load_project_name(path: &Path) -> Result<String> {
    let conn = Connection::open(path)?;
    Ok(load_meta(&conn, "name")?.unwrap_or_else(|| display_name_from_path(path)))
}

fn load_track_pedals(conn: &Connection, track_position: i64) -> Result<Vec<PedalSpec>> {
    let mut statement = conn.prepare("SELECT data_json FROM pedals WHERE track_position = ?1 ORDER BY position ASC")?;
    let mut rows = statement.query(params![track_position])?;
    let mut pedals = Vec::new();
    while let Some(row) = rows.next()? {
        let data: String = row.get(0)?;
        pedals.push(serde_json::from_str(&data).context("failed to decode pedal json")?);
    }
    Ok(pedals)
}

fn insert_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![key, value])?;
    Ok(())
}

fn load_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut statement = conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = statement.query(params![key])?;
    Ok(rows.next()?.map(|row| row.get(0)).transpose()?)
}

fn display_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled")
        .trim_end_matches(".lupe")
        .replace('-', " ")
}

fn default_file_name(name: &str) -> String {
    format!("{}.{}", slug(name), PROJECT_EXT)
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

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}
