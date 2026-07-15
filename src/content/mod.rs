use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::BaseDirs;
use midly::{MidiMessage, Smf, Timing, TrackEventKind};
use serde::{Deserialize, Serialize};

use crate::project::{DrumHumanize, DrumLane, DrumRole, DrumSequence, DrumStepSettings, SequencerSubdivision};

#[derive(Clone, Debug)]
pub struct ContentPaths {
    pub root: PathBuf,
    pub patterns: PathBuf,
    pub user_patterns: PathBuf,
    pub kits: PathBuf,
    pub user_kits: PathBuf,
    pub downloads: PathBuf,
    pub cache: PathBuf,
}

impl ContentPaths {
    fn discover() -> Result<Self> {
        let base = BaseDirs::new().context("could not determine the user data directory")?;
        let root = base.data_dir().join("lupe");
        Ok(Self {
            patterns: root.join("patterns"),
            user_patterns: root.join("patterns/user"),
            kits: root.join("kits"),
            user_kits: root.join("kits/user"),
            downloads: root.join("downloads"),
            cache: root.join("cache"),
            root,
        })
    }

    fn ensure(&self) -> Result<()> {
        for path in [
            &self.root,
            &self.patterns,
            &self.user_patterns,
            &self.kits,
            &self.user_kits,
            &self.downloads,
            &self.cache,
        ] {
            fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ContentPack {
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct ContentRegistry {
    paths: ContentPaths,
    pattern_packs: Vec<ContentPack>,
    kit_packs: Vec<ContentPack>,
    patterns: Vec<PatternAsset>,
}

impl ContentRegistry {
    pub fn discover() -> Result<Self> {
        let paths = ContentPaths::discover()?;
        paths.ensure()?;
        let mut registry = Self {
            paths,
            pattern_packs: Vec::new(),
            kit_packs: Vec::new(),
            patterns: Vec::new(),
        };
        registry.rescan();
        Ok(registry)
    }

    pub fn paths(&self) -> &ContentPaths {
        &self.paths
    }

    pub fn kit_packs(&self) -> &[ContentPack] {
        &self.kit_packs
    }

    pub fn patterns(&self) -> &[PatternAsset] {
        &self.patterns
    }

    pub fn load_pattern(&self, id: &str) -> Result<DrumSequence> {
        let asset = self.patterns.iter().find(|asset| asset.id == id).context("pattern is no longer installed")?;
        match &asset.source {
            PatternSource::Midi(path) => import_midi_pattern(path, asset),
            PatternSource::Native(path) => {
                let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
                let file: NativePatternFile = serde_json::from_slice(&bytes)
                    .with_context(|| format!("failed to parse {}", path.display()))?;
                anyhow::ensure!(file.schema_version == 1, "unsupported pattern schema {}", file.schema_version);
                Ok(file.sequence)
            }
        }
    }

    pub fn save_user_pattern(
        &mut self,
        name: &str,
        sequence: &DrumSequence,
        kind: PatternKind,
        bpm: u32,
        beats_per_bar: u32,
    ) -> Result<()> {
        let slug = name
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug = if slug.is_empty() { "pattern" } else { &slug };
        let mut path = self.paths.user_patterns.join(format!("{slug}.lupe-pattern.json"));
        let mut suffix = 2;
        while path.exists() {
            path = self.paths.user_patterns.join(format!("{slug}-{suffix}.lupe-pattern.json"));
            suffix += 1;
        }
        let file = NativePatternFile {
            schema_version: 1,
            name: name.trim().to_string(),
            kind,
            style: "user".to_string(),
            bpm: bpm.max(1),
            beats_per_bar: beats_per_bar.max(1),
            sequence: sequence.clone(),
        };
        let json = serde_json::to_vec_pretty(&file)?;
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, json).with_context(|| format!("failed to write {}", temporary.display()))?;
        fs::rename(&temporary, &path).with_context(|| format!("failed to install {}", path.display()))?;
        self.rescan();
        Ok(())
    }

    pub fn rescan(&mut self) {
        self.pattern_packs = scan_pack_root(&self.paths.patterns, ContentKind::Pattern);
        self.kit_packs = scan_pack_root(&self.paths.kits, ContentKind::Kit);
        self.patterns = scan_patterns(&self.paths.patterns, &self.paths.user_patterns);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternKind {
    Loop,
    Fill,
}

impl PatternKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Loop => "Loop",
            Self::Fill => "Fill",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PatternAsset {
    pub id: String,
    pub name: String,
    pub style: String,
    pub description: String,
    pub bpm: u32,
    pub beats_per_bar: u32,
    pub kind: PatternKind,
    pub featured: bool,
    pub user_owned: bool,
    source: PatternSource,
}

#[derive(Clone, Debug)]
enum PatternSource {
    Midi(PathBuf),
    Native(PathBuf),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NativePatternFile {
    schema_version: u32,
    name: String,
    kind: PatternKind,
    style: String,
    bpm: u32,
    #[serde(default = "default_beats_per_bar")]
    beats_per_bar: u32,
    sequence: DrumSequence,
}

fn default_beats_per_bar() -> u32 {
    4
}

#[derive(Clone, Copy)]
enum ContentKind {
    Pattern,
    Kit,
}

fn scan_pack_root(root: &Path, kind: ContentKind) -> Vec<ContentPack> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut packs = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let item_count = count_content_files(&path, kind);
            if item_count == 0 {
                return None;
            }
            Some(ContentPack {
                id: entry.file_name().to_string_lossy().to_string(),
            })
        })
        .collect::<Vec<_>>();
    packs.sort_by(|left, right| left.id.cmp(&right.id));
    packs
}

fn count_content_files(path: &Path, kind: ContentKind) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                count_content_files(&path, kind)
            } else if content_extension_matches(&path, kind) {
                1
            } else {
                0
            }
        })
        .sum()
}

fn content_extension_matches(path: &Path, kind: ContentKind) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    match kind {
        ContentKind::Pattern => matches!(extension.as_str(), "mid" | "midi"),
        ContentKind::Kit => extension == "wav",
    }
}

fn scan_patterns(root: &Path, user_root: &Path) -> Vec<PatternAsset> {
    let mut patterns = Vec::new();
    let mut info_files = Vec::new();
    collect_named_files(root, "info.csv", &mut info_files);
    for info_path in info_files {
        let Some(dataset_root) = info_path.parent() else {
            continue;
        };
        let Ok(contents) = fs::read_to_string(&info_path) else {
            continue;
        };
        for line in contents.lines().skip(1) {
            let fields = line.split(',').collect::<Vec<_>>();
            if fields.len() < 11 {
                continue;
            }
            let kind = match fields[5] {
                "beat" => PatternKind::Loop,
                "fill" => PatternKind::Fill,
                _ => continue,
            };
            let mut meter = fields[6].split('-');
            let beats_per_bar = meter.next().and_then(|value| value.parse().ok()).unwrap_or(4);
            if beats_per_bar != 4 {
                continue;
            }
            let midi_path = dataset_root.join(fields[7]);
            if !midi_path.is_file() {
                continue;
            }
            let style = fields[3].split('/').next().unwrap_or("groove").to_string();
            let bpm = fields[4].parse().unwrap_or(110);
            patterns.push(PatternAsset {
                id: format!("gmd:{}", fields[2]),
                name: pattern_display_name(fields[3], kind),
                style,
                description: pattern_description(fields[3], kind, bpm),
                bpm,
                beats_per_bar,
                kind,
                featured: false,
                user_owned: false,
                source: PatternSource::Midi(midi_path),
            });
        }
    }

    let mut native_files = Vec::new();
    collect_extension_files(user_root, "json", &mut native_files);
    for path in native_files {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(file) = serde_json::from_slice::<NativePatternFile>(&bytes) else {
            continue;
        };
        if file.schema_version != 1 {
            continue;
        }
        patterns.push(PatternAsset {
            id: format!("user:{}", path.to_string_lossy()),
            name: file.name,
            style: file.style,
            description: "A reusable pattern saved from one of your projects.".to_string(),
            bpm: file.bpm,
            beats_per_bar: file.beats_per_bar.max(1),
            kind: file.kind,
            featured: true,
            user_owned: true,
            source: PatternSource::Native(path),
        });
    }

    patterns.sort_by(|left, right| {
        left.user_owned
            .cmp(&right.user_owned)
            .reverse()
            .then_with(|| left.kind.label().cmp(right.kind.label()))
            .then_with(|| left.style.cmp(&right.style))
            .then_with(|| left.id.cmp(&right.id))
    });
    mark_featured(&mut patterns, PatternKind::Loop, 72);
    mark_featured(&mut patterns, PatternKind::Fill, 48);
    patterns
}

fn mark_featured(patterns: &mut [PatternAsset], kind: PatternKind, target: usize) {
    let mut styles = patterns
        .iter()
        .filter(|pattern| pattern.kind == kind && !pattern.user_owned)
        .map(|pattern| pattern.style.clone())
        .collect::<Vec<_>>();
    styles.sort();
    styles.dedup();
    let mut selected = 0;
    let mut pass = 0;
    while selected < target {
        let mut changed = false;
        for style in &styles {
            let candidate = patterns
                .iter_mut()
                .filter(|pattern| pattern.kind == kind && pattern.style == *style && !pattern.user_owned)
                .nth(pass);
            if let Some(pattern) = candidate {
                pattern.featured = true;
                selected += 1;
                changed = true;
                if selected == target {
                    break;
                }
            }
        }
        if !changed {
            break;
        }
        pass += 1;
    }
}

fn collect_named_files(path: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_named_files(&path, name, out);
        } else if path.file_name().and_then(|value| value.to_str()) == Some(name) {
            out.push(path);
        }
    }
}

fn collect_extension_files(path: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_extension_files(&path, extension, out);
        } else if path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension)) {
            out.push(path);
        }
    }
}

fn pattern_display_name(style: &str, kind: PatternKind) -> String {
    let mut parts = style.split('/');
    let family = title_case(parts.next().unwrap_or("groove"));
    let detail = parts.next().map(title_case).unwrap_or_else(|| kind.label().to_string());
    format!("{family} · {detail}")
}

fn pattern_description(style: &str, kind: PatternKind, bpm: u32) -> String {
    let family = style.split('/').next().unwrap_or("drum");
    let pace = if bpm < 85 {
        "slow"
    } else if bpm < 125 {
        "mid-tempo"
    } else {
        "fast"
    };
    match kind {
        PatternKind::Loop => format!("A human-played {pace} {family} groove. Edit any hit or map it to another sound."),
        PatternKind::Fill => format!("A human-played {pace} {family} transition for the end of a phrase."),
    }
}

fn title_case(value: &str) -> String {
    value
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn import_midi_pattern(path: &Path, asset: &PatternAsset) -> Result<DrumSequence> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let smf = Smf::parse(&bytes).with_context(|| format!("failed to parse {}", path.display()))?;
    let ticks_per_beat = match smf.header.timing {
        Timing::Metrical(value) => value.as_int() as u64,
        Timing::Timecode(_, _) => anyhow::bail!("timecode MIDI is not supported"),
    };
    let mut notes = Vec::new();
    for track in &smf.tracks {
        let mut tick = 0_u64;
        for event in track {
            tick = tick.saturating_add(event.delta.as_int() as u64);
            if let TrackEventKind::Midi {
                message: MidiMessage::NoteOn { key, vel },
                ..
            } = event.kind
            {
                if vel.as_int() > 0 {
                    notes.push((tick, key.as_int(), vel.as_int()));
                }
            }
        }
    }
    notes.sort_by_key(|note| note.0);
    let first_tick = notes.first().map(|note| note.0).unwrap_or(0);
    let ticks_per_bar = ticks_per_beat * asset.beats_per_bar.max(1) as u64;
    let bar_start = (first_tick / ticks_per_bar) * ticks_per_bar;
    let remainder = first_tick - bar_start;
    let early_downbeat_window = ticks_per_beat / 2;
    let start_tick = if remainder + early_downbeat_window >= ticks_per_bar {
        bar_start + ticks_per_bar
    } else {
        bar_start
    };
    let measures = if asset.kind == PatternKind::Fill { 1 } else { 2 };
    let end_tick = start_tick + ticks_per_bar * measures as u64;
    let total_steps = measures as usize * asset.beats_per_bar as usize * 4;
    let mut lanes = Vec::<DrumLane>::new();

    for (tick, note, velocity) in notes {
        if tick.saturating_add(early_downbeat_window) < start_tick || tick >= end_tick {
            continue;
        }
        let role = midi_note_role(note);
        let lane_index = lanes.iter().position(|lane| lane.role == role).unwrap_or_else(|| {
            let mut lane = DrumLane::new(role.label(), None);
            lane.role = role;
            lane.steps.resize(total_steps, false);
            lane.step_settings.resize(total_steps, DrumStepSettings::default());
            lanes.push(lane);
            lanes.len() - 1
        });
        let exact_step = (tick as i128 - start_tick as i128) as f64 / ticks_per_beat as f64 * 4.0;
        let step_index = exact_step.round().clamp(0.0, total_steps.saturating_sub(1) as f64) as usize;
        let offset_steps = (exact_step - step_index as f64) as f32;
        let setting = DrumStepSettings {
            velocity: (velocity as f32 / 127.0).clamp(0.08, 1.0),
            probability: 1.0,
            offset_steps: offset_steps.clamp(-0.49, 0.49),
        };
        let lane = &mut lanes[lane_index];
        if !lane.steps[step_index] || setting.velocity > lane.step_settings[step_index].velocity {
            lane.steps[step_index] = true;
            lane.step_settings[step_index] = setting;
        }
    }

    trim_leading_empty_bars(&mut lanes, asset.beats_per_bar as usize * 4);

    lanes.sort_by_key(|lane| role_order(lane.role));
    Ok(DrumSequence {
        measures,
        subdivision: SequencerSubdivision::Sixteenth,
        lanes,
        humanize: DrumHumanize {
            timing_ms: 0.0,
            velocity_variation: 0.0,
            swing: 0.0,
            feel_ms: 0.0,
            seed: hash_text(&asset.id),
            evolving: false,
        },
    })
}

fn trim_leading_empty_bars(lanes: &mut [DrumLane], steps_per_bar: usize) {
    let Some(first_hit) = lanes
        .iter()
        .flat_map(|lane| lane.steps.iter().enumerate())
        .filter_map(|(index, enabled)| enabled.then_some(index))
        .min()
    else {
        return;
    };
    let shift = (first_hit / steps_per_bar.max(1)) * steps_per_bar;
    if shift == 0 {
        return;
    }
    for lane in lanes {
        lane.steps.rotate_left(shift);
        lane.step_settings.rotate_left(shift);
        let len = lane.steps.len();
        lane.steps[len - shift..].fill(false);
        lane.step_settings[len - shift..].fill(DrumStepSettings::default());
    }
}

fn midi_note_role(note: u8) -> DrumRole {
    match note {
        35 | 36 => DrumRole::Kick,
        37 => DrumRole::Percussion,
        38..=40 => DrumRole::Snare,
        22 | 42 => DrumRole::ClosedHat,
        44 => DrumRole::PedalHat,
        26 | 46 => DrumRole::OpenHat,
        48 | 50 => DrumRole::HighTom,
        45 | 47 => DrumRole::MidTom,
        41 | 43 => DrumRole::FloorTom,
        51 | 53 | 59 => DrumRole::Ride,
        49 | 52 | 55 | 57 => DrumRole::Crash,
        _ => DrumRole::Percussion,
    }
}

fn role_order(role: DrumRole) -> u8 {
    match role {
        DrumRole::Kick => 0,
        DrumRole::Snare => 1,
        DrumRole::ClosedHat => 2,
        DrumRole::OpenHat => 3,
        DrumRole::PedalHat => 4,
        DrumRole::HighTom => 5,
        DrumRole::MidTom => 6,
        DrumRole::FloorTom => 7,
        DrumRole::Ride => 8,
        DrumRole::Crash => 9,
        DrumRole::Percussion => 10,
        DrumRole::Other => 11,
    }
}

fn hash_text(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x1000_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_velocity_and_positions_from_drum_midi() {
        let path = std::env::temp_dir().join(format!("lupe-pattern-{}.mid", std::process::id()));
        let midi = [
            b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 0, 0, 1, 1, 0xe0, b'M', b'T', b'r', b'k', 0, 0, 0, 13,
            0, 0x99, 36, 100, 0x83, 0x60, 0x99, 38, 80, 0, 0xff, 0x2f, 0,
        ];
        fs::write(&path, midi).unwrap();
        let asset = PatternAsset {
            id: "test:groove".to_string(),
            name: "Test Groove".to_string(),
            style: "rock".to_string(),
            description: String::new(),
            bpm: 120,
            beats_per_bar: 4,
            kind: PatternKind::Loop,
            featured: true,
            user_owned: false,
            source: PatternSource::Midi(path.clone()),
        };
        let sequence = import_midi_pattern(&path, &asset).unwrap();
        let kick = sequence.lanes.iter().find(|lane| lane.role == DrumRole::Kick).unwrap();
        let snare = sequence.lanes.iter().find(|lane| lane.role == DrumRole::Snare).unwrap();
        assert!(kick.steps[0]);
        assert!(snare.steps[4]);
        assert!((kick.step_settings[0].velocity - 100.0 / 127.0).abs() < 0.001);
        assert!((snare.step_settings[4].velocity - 80.0 / 127.0).abs() < 0.001);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn maps_general_midi_drum_roles() {
        assert_eq!(midi_note_role(36), DrumRole::Kick);
        assert_eq!(midi_note_role(37), DrumRole::Percussion);
        assert_eq!(midi_note_role(38), DrumRole::Snare);
        assert_eq!(midi_note_role(42), DrumRole::ClosedHat);
        assert_eq!(midi_note_role(46), DrumRole::OpenHat);
        assert_eq!(midi_note_role(51), DrumRole::Ride);
        assert_eq!(midi_note_role(58), DrumRole::Percussion);
    }

    #[test]
    fn slightly_early_downbeat_does_not_create_an_empty_leading_bar() {
        let path = std::env::temp_dir().join(format!("lupe-early-pattern-{}.mid", std::process::id()));
        let midi = [
            b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 0, 0, 1, 1, 0xe0, b'M', b'T', b'r', b'k', 0, 0, 0, 9,
            0x8e, 0x08, 0x99, 36, 100, 0, 0xff, 0x2f, 0,
        ];
        fs::write(&path, midi).unwrap();
        let asset = PatternAsset {
            id: "test:early-downbeat".to_string(),
            name: "Early Downbeat".to_string(),
            style: "rock".to_string(),
            description: String::new(),
            bpm: 120,
            beats_per_bar: 4,
            kind: PatternKind::Loop,
            featured: true,
            user_owned: false,
            source: PatternSource::Midi(path.clone()),
        };

        let sequence = import_midi_pattern(&path, &asset).unwrap();
        let kick = sequence.lanes.iter().find(|lane| lane.role == DrumRole::Kick).unwrap();
        assert!(kick.steps[0]);
        assert!(kick.step_settings[0].offset_steps < 0.0);
        let _ = fs::remove_file(path);
    }
}
