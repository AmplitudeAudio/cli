//! Sound asset type.
//!
//! Represents a single audio source with playback configuration for the
//! Amplitude Audio SDK. Sounds are the fundamental building blocks of audio
//! in a project, referencing audio files in the data/ directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

// =============================================================================
// Spatialization Enum
// =============================================================================

/// Defines how the sound will be rendered in 3D space.
///
/// Controls spatial audio effects like sound attenuation and panning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Spatialization {
    /// No spatial rendering applied.
    #[default]
    None,
    /// 2D spatialization using sound source position only.
    Position,
    /// 2D spatialization using position and orientation.
    PositionOrientation,
    /// 3D spatialization via HRIR Sphere asset.
    #[serde(rename = "HRTF")]
    Hrtf,
}

// =============================================================================
// Scope Enum
// =============================================================================

/// Controls how playback data is shared between sound instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scope {
    /// All instances treated as single object with shared data.
    #[default]
    World,
    /// Each instance per entity with intra-entity data sharing.
    Entity,
}

// =============================================================================
// FaderAlgorithm Enum
// =============================================================================

/// Algorithm used for fade-in/fade-out animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FaderAlgorithm {
    /// Constant fader (no transition).
    Constant,
    /// Ease fader.
    Ease,
    /// Ease-in fader.
    EaseIn,
    /// Ease-in-out fader.
    EaseInOut,
    /// Ease-out fader.
    EaseOut,
    /// Linear fader (default).
    #[default]
    Linear,
    /// Smooth S-curve fader.
    SCurveSmooth,
    /// Sharp S-curve fader.
    SCurveSharp,
}

// =============================================================================
// RtpcCompatibleValue
// =============================================================================

/// Value that can be either static or controlled by RTPC.
///
/// RTPC (Real-Time Parameter Control) allows values to be synchronized
/// between the game and Amplitude at runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RtpcCompatibleValue {
    /// A static, constant value.
    Static {
        /// The fixed value.
        value: f32,
    },
    /// A dynamic value controlled by RTPC.
    #[serde(rename = "RTPC")]
    Rtpc {
        /// RTPC configuration for dynamic control.
        rtpc: RtpcReference,
    },
}

impl Default for RtpcCompatibleValue {
    fn default() -> Self {
        Self::Static { value: 1.0 }
    }
}

impl RtpcCompatibleValue {
    /// Creates a static value.
    pub fn static_value(value: f32) -> Self {
        Self::Static { value }
    }

    /// Creates an RTPC-controlled value.
    pub fn rtpc(id: u64, curve: CurveDefinition) -> Self {
        Self::Rtpc {
            rtpc: RtpcReference { id, curve },
        }
    }

    /// Returns the static value if this is a Static variant.
    pub fn as_static(&self) -> Option<f32> {
        match self {
            Self::Static { value } => Some(*value),
            Self::Rtpc { .. } => None,
        }
    }
}

/// Reference to an RTPC object with curve mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RtpcReference {
    /// The ID of the RTPC object.
    pub id: u64,
    /// The curve to use for value mapping.
    pub curve: CurveDefinition,
}

/// Defines a curve for value mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveDefinition {
    /// The curve parts that define the mapping.
    pub parts: Vec<CurvePart>,
}

/// A segment of a curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurvePart {
    /// Start point of this curve segment.
    pub start: CurvePoint,
    /// End point of this curve segment.
    pub end: CurvePoint,
    /// Fader algorithm for interpolation.
    pub fader: FaderAlgorithm,
}

/// A point on a curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    /// X coordinate (input value).
    pub x: f32,
    /// Y coordinate (output value).
    pub y: f32,
}

// =============================================================================
// SoundLoopConfig
// =============================================================================

/// Configuration for sound looping behavior.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SoundLoopConfig {
    /// Whether looping is enabled.
    pub enabled: bool,
    /// Number of times to loop (0 = infinite).
    #[serde(default)]
    pub loop_count: u32,
}

impl SoundLoopConfig {
    /// Creates a disabled loop configuration.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            loop_count: 0,
        }
    }

    /// Creates an infinite loop configuration.
    pub fn infinite() -> Self {
        Self {
            enabled: true,
            loop_count: 0,
        }
    }

    /// Creates a finite loop configuration.
    pub fn count(times: u32) -> Self {
        Self {
            enabled: true,
            loop_count: times,
        }
    }
}

// =============================================================================
// Sound Struct
// =============================================================================

/// Individual sound definition.
///
/// Represents a single audio source with playback configuration.
/// Sounds reference audio files in the project's `data/` directory.
///
/// # Example
///
/// ```
/// use am::assets::{Sound, Spatialization, RtpcCompatibleValue};
///
/// let sound = Sound::builder(12345, "explosion")
///     .path("sfx/explosion_01.wav")
///     .gain(0.8)
///     .priority(200)
///     .build();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sound {
    /// Unique identifier for this sound.
    pub id: u64,

    /// Name of the sound.
    pub name: String,

    /// Path to the audio file relative to the project's `data/` directory.
    pub path: PathBuf,

    /// Bus ID for routing this sound (0 = master bus).
    #[serde(default)]
    pub bus: u64,

    /// Gain/volume control (default: 1.0 static).
    #[serde(default)]
    pub gain: RtpcCompatibleValue,

    /// Playback priority (default: 128 static, higher = more important).
    #[serde(default = "default_priority")]
    pub priority: RtpcCompatibleValue,

    /// Whether to stream from disk vs load into memory.
    #[serde(default)]
    pub stream: bool,

    /// Looping configuration.
    #[serde(rename = "loop", default)]
    pub loop_config: SoundLoopConfig,

    /// How the sound is rendered in 3D space.
    #[serde(default)]
    pub spatialization: Spatialization,

    /// Attenuation model ID (0 = none).
    #[serde(default)]
    pub attenuation: u64,

    /// How playback data is shared between instances.
    #[serde(default)]
    pub scope: Scope,

    /// Fader algorithm for fade-in/fade-out.
    #[serde(default)]
    pub fader: FaderAlgorithm,

    /// Effect ID to apply (0 = none).
    #[serde(default)]
    pub effect: u64,
}

fn default_priority() -> RtpcCompatibleValue {
    RtpcCompatibleValue::Static { value: 128.0 }
}

impl Sound {
    /// Creates a new Sound with required fields only.
    ///
    /// Use `Sound::builder()` for more control over optional fields.
    pub fn new(id: u64, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id,
            name: name.into(),
            path: path.into(),
            bus: 0,
            gain: RtpcCompatibleValue::default(),
            priority: default_priority(),
            stream: false,
            loop_config: SoundLoopConfig::default(),
            spatialization: Spatialization::default(),
            attenuation: 0,
            scope: Scope::default(),
            fader: FaderAlgorithm::default(),
            effect: 0,
        }
    }

    /// Creates a builder for constructing a Sound with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> SoundBuilder {
        SoundBuilder::new(id, name)
    }
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            path: PathBuf::new(),
            bus: 0,
            gain: RtpcCompatibleValue::default(),
            priority: default_priority(),
            stream: false,
            loop_config: SoundLoopConfig::default(),
            spatialization: Spatialization::default(),
            attenuation: 0,
            scope: Scope::default(),
            fader: FaderAlgorithm::default(),
            effect: 0,
        }
    }
}

// =============================================================================
// SoundBuilder
// =============================================================================

/// Builder for constructing Sound instances with optional fields.
#[derive(Debug, Clone)]
pub struct SoundBuilder {
    sound: Sound,
}

impl SoundBuilder {
    /// Creates a new SoundBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            sound: Sound {
                id,
                name: name.into(),
                path: PathBuf::new(),
                bus: 0,
                gain: RtpcCompatibleValue::default(),
                priority: default_priority(),
                stream: false,
                loop_config: SoundLoopConfig::default(),
                spatialization: Spatialization::default(),
                attenuation: 0,
                scope: Scope::default(),
                fader: FaderAlgorithm::default(),
                effect: 0,
            },
        }
    }

    /// Sets the path to the audio file.
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.sound.path = path.into();
        self
    }

    /// Sets the bus ID.
    pub fn bus(mut self, bus_id: u64) -> Self {
        self.sound.bus = bus_id;
        self
    }

    /// Sets the gain as a static value.
    pub fn gain(mut self, value: f32) -> Self {
        self.sound.gain = RtpcCompatibleValue::static_value(value);
        self
    }

    /// Sets the gain with full RtpcCompatibleValue control.
    pub fn gain_rtpc(mut self, value: RtpcCompatibleValue) -> Self {
        self.sound.gain = value;
        self
    }

    /// Sets the priority as a static value.
    pub fn priority(mut self, value: u8) -> Self {
        self.sound.priority = RtpcCompatibleValue::static_value(value as f32);
        self
    }

    /// Sets the priority with full RtpcCompatibleValue control.
    pub fn priority_rtpc(mut self, value: RtpcCompatibleValue) -> Self {
        self.sound.priority = value;
        self
    }

    /// Enables streaming from disk.
    pub fn stream(mut self, stream: bool) -> Self {
        self.sound.stream = stream;
        self
    }

    /// Sets the loop configuration.
    pub fn loop_config(mut self, config: SoundLoopConfig) -> Self {
        self.sound.loop_config = config;
        self
    }

    /// Sets the spatialization mode.
    pub fn spatialization(mut self, mode: Spatialization) -> Self {
        self.sound.spatialization = mode;
        self
    }

    /// Sets the attenuation model ID.
    pub fn attenuation(mut self, attenuation_id: u64) -> Self {
        self.sound.attenuation = attenuation_id;
        self
    }

    /// Sets the scope mode.
    pub fn scope(mut self, scope: Scope) -> Self {
        self.sound.scope = scope;
        self
    }

    /// Sets the fader algorithm.
    pub fn fader(mut self, fader: FaderAlgorithm) -> Self {
        self.sound.fader = fader;
        self
    }

    /// Sets the effect ID.
    pub fn effect(mut self, effect_id: u64) -> Self {
        self.sound.effect = effect_id;
        self
    }

    /// Builds the Sound instance.
    pub fn build(self) -> Sound {
        self.sound
    }
}

impl Asset for Sound {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Sound
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Sound.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 6
        Ok(())
    }

    fn validate_rules(&self, context: &ProjectContext) -> Result<(), ValidationError> {
        // Check audio file path exists
        if !self.path.as_os_str().is_empty() {
            let audio_path = context.project_root.join("data").join(&self.path);
            if !audio_path.exists() {
                return Err(ValidationError::type_rule_violation(
                    format!("Audio file not found: {}", self.path.display()),
                    "Sound assets must reference an existing audio file in the data/ directory",
                )
                .with_suggestion(format!(
                    "Add the audio file at: {}",
                    audio_path.display()
                ))
                .with_field("path"));
            }
        }

        // Check gain range (if static value)
        if let Some(gain_value) = self.gain.as_static()
            && !(0.0..=1.0).contains(&gain_value)
        {
            return Err(ValidationError::type_rule_violation(
                format!("Invalid gain value: {}", gain_value),
                "Gain must be between 0.0 and 1.0",
            )
            .with_suggestion("Set gain to a value between 0.0 (silent) and 1.0 (full volume)")
            .with_field("gain"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_new() {
        let sound = Sound::new(12345, "explosion", "sfx/explosion.wav");
        assert_eq!(sound.id(), 12345);
        assert_eq!(sound.name(), "explosion");
        assert_eq!(sound.path, PathBuf::from("sfx/explosion.wav"));
    }

    #[test]
    fn test_sound_default() {
        let sound = Sound::default();
        assert_eq!(sound.id, 0);
        assert_eq!(sound.name, "");
        assert_eq!(sound.path, PathBuf::new());
        assert_eq!(sound.bus, 0);
        assert_eq!(sound.gain.as_static(), Some(1.0));
        assert_eq!(sound.priority.as_static(), Some(128.0));
        assert!(!sound.stream);
        assert!(!sound.loop_config.enabled);
        assert_eq!(sound.spatialization, Spatialization::None);
        assert_eq!(sound.scope, Scope::World);
        assert_eq!(sound.fader, FaderAlgorithm::Linear);
    }

    #[test]
    fn test_sound_builder() {
        let sound = Sound::builder(12345, "explosion")
            .path("sfx/explosion.wav")
            .bus(100)
            .gain(0.8)
            .priority(200)
            .stream(true)
            .loop_config(SoundLoopConfig::infinite())
            .spatialization(Spatialization::Position)
            .attenuation(50)
            .scope(Scope::Entity)
            .fader(FaderAlgorithm::SCurveSmooth)
            .effect(25)
            .build();

        assert_eq!(sound.id, 12345);
        assert_eq!(sound.name, "explosion");
        assert_eq!(sound.path, PathBuf::from("sfx/explosion.wav"));
        assert_eq!(sound.bus, 100);
        assert_eq!(sound.gain.as_static(), Some(0.8));
        assert_eq!(sound.priority.as_static(), Some(200.0));
        assert!(sound.stream);
        assert!(sound.loop_config.enabled);
        assert_eq!(sound.loop_config.loop_count, 0);
        assert_eq!(sound.spatialization, Spatialization::Position);
        assert_eq!(sound.attenuation, 50);
        assert_eq!(sound.scope, Scope::Entity);
        assert_eq!(sound.fader, FaderAlgorithm::SCurveSmooth);
        assert_eq!(sound.effect, 25);
    }

    #[test]
    fn test_sound_asset_type() {
        let sound = Sound::new(1, "test", "test.wav");
        assert_eq!(sound.asset_type(), AssetType::Sound);
    }

    #[test]
    fn test_sound_file_extension() {
        let sound = Sound::new(1, "test", "test.wav");
        assert_eq!(sound.file_extension(), ".json");
    }

    #[test]
    fn test_sound_serde_roundtrip() {
        let sound = Sound::builder(12345, "explosion")
            .path("sfx/explosion.wav")
            .bus(100)
            .gain(0.8)
            .priority(200)
            .stream(true)
            .loop_config(SoundLoopConfig::count(3))
            .spatialization(Spatialization::PositionOrientation)
            .attenuation(50)
            .scope(Scope::Entity)
            .fader(FaderAlgorithm::EaseInOut)
            .effect(25)
            .build();

        let json = serde_json::to_string(&sound).unwrap();
        let parsed: Sound = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, sound.id);
        assert_eq!(parsed.name, sound.name);
        assert_eq!(parsed.path, sound.path);
        assert_eq!(parsed.bus, sound.bus);
        assert_eq!(parsed.gain, sound.gain);
        assert_eq!(parsed.priority, sound.priority);
        assert_eq!(parsed.stream, sound.stream);
        assert_eq!(parsed.loop_config, sound.loop_config);
        assert_eq!(parsed.spatialization, sound.spatialization);
        assert_eq!(parsed.attenuation, sound.attenuation);
        assert_eq!(parsed.scope, sound.scope);
        assert_eq!(parsed.fader, sound.fader);
        assert_eq!(parsed.effect, sound.effect);
    }

    #[test]
    fn test_sound_serde_sdk_format() {
        // Test JSON output matches SDK format expectations
        let sound = Sound::builder(12345, "forest_ambient")
            .path("ambient/forest.ogg")
            .bus(200)
            .gain(0.75)
            .priority(128)
            .stream(true)
            .loop_config(SoundLoopConfig::infinite())
            .spatialization(Spatialization::Hrtf)
            .scope(Scope::Entity)
            .fader(FaderAlgorithm::SCurveSmooth)
            .build();

        let json = serde_json::to_string_pretty(&sound).unwrap();

        // Verify key field names are in SDK format
        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"forest_ambient\""));
        assert!(json.contains("\"path\": \"ambient/forest.ogg\""));
        assert!(json.contains("\"bus\": 200"));
        assert!(json.contains("\"stream\": true"));
        assert!(json.contains("\"loop\":"));
        assert!(json.contains("\"enabled\": true"));
        assert!(json.contains("\"loop_count\": 0"));
        assert!(json.contains("\"spatialization\": \"HRTF\""));
        assert!(json.contains("\"scope\": \"Entity\""));
        assert!(json.contains("\"fader\": \"SCurveSmooth\""));
        assert!(json.contains("\"kind\": \"Static\""));
    }

    #[test]
    fn test_sound_deserialize_sdk_json() {
        // Test deserialization from SDK-format JSON
        let sdk_json = r#"{
            "id": 54321,
            "name": "footstep",
            "path": "sfx/footstep_01.wav",
            "bus": 100,
            "gain": { "kind": "Static", "value": 0.8 },
            "priority": { "kind": "Static", "value": 128.0 },
            "stream": false,
            "loop": { "enabled": false, "loop_count": 0 },
            "spatialization": "Position",
            "attenuation": 0,
            "scope": "World",
            "fader": "Linear",
            "effect": 0
        }"#;

        let sound: Sound = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(sound.id, 54321);
        assert_eq!(sound.name, "footstep");
        assert_eq!(sound.path, PathBuf::from("sfx/footstep_01.wav"));
        assert_eq!(sound.bus, 100);
        assert_eq!(sound.gain.as_static(), Some(0.8));
        assert_eq!(sound.priority.as_static(), Some(128.0));
        assert!(!sound.stream);
        assert!(!sound.loop_config.enabled);
        assert_eq!(sound.spatialization, Spatialization::Position);
        assert_eq!(sound.scope, Scope::World);
        assert_eq!(sound.fader, FaderAlgorithm::Linear);
    }

    #[test]
    fn test_sound_validate_rules_passes_valid() {
        use tempfile::tempdir;
        use std::fs;

        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().join("data").join("sfx");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

        let context = ProjectContext::new(temp_dir.path().to_path_buf());
        let sound = Sound::builder(1, "explosion")
            .path("sfx/explosion.wav")
            .gain(0.8)
            .build();

        assert!(sound.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_sound_validate_rules_fails_missing_file() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let context = ProjectContext::new(temp_dir.path().to_path_buf());

        let sound = Sound::builder(1, "explosion")
            .path("sfx/missing.wav")
            .gain(0.8)
            .build();

        let result = sound.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("Audio file not found"));
        assert_eq!(err.field, Some("path".to_string()));
    }

    #[test]
    fn test_sound_validate_rules_fails_invalid_gain() {
        let context = ProjectContext::empty();

        // Test gain > 1.0
        let sound = Sound::builder(1, "test")
            .path("") // Empty path to skip file check
            .gain(1.5)
            .build();

        let result = sound.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("Invalid gain value"));
        assert_eq!(err.field, Some("gain".to_string()));

        // Test gain < 0.0
        let sound = Sound::builder(2, "test2")
            .path("")
            .gain(-0.5)
            .build();

        let result = sound.validate_rules(&context);
        assert!(result.is_err());
    }

    #[test]
    fn test_spatialization_serde() {
        assert_eq!(
            serde_json::to_string(&Spatialization::None).unwrap(),
            "\"None\""
        );
        assert_eq!(
            serde_json::to_string(&Spatialization::Position).unwrap(),
            "\"Position\""
        );
        assert_eq!(
            serde_json::to_string(&Spatialization::PositionOrientation).unwrap(),
            "\"PositionOrientation\""
        );
        assert_eq!(
            serde_json::to_string(&Spatialization::Hrtf).unwrap(),
            "\"HRTF\""
        );
    }

    #[test]
    fn test_scope_serde() {
        assert_eq!(serde_json::to_string(&Scope::World).unwrap(), "\"World\"");
        assert_eq!(serde_json::to_string(&Scope::Entity).unwrap(), "\"Entity\"");
    }

    #[test]
    fn test_fader_algorithm_serde() {
        assert_eq!(
            serde_json::to_string(&FaderAlgorithm::Linear).unwrap(),
            "\"Linear\""
        );
        assert_eq!(
            serde_json::to_string(&FaderAlgorithm::SCurveSmooth).unwrap(),
            "\"SCurveSmooth\""
        );
        assert_eq!(
            serde_json::to_string(&FaderAlgorithm::EaseInOut).unwrap(),
            "\"EaseInOut\""
        );
    }

    #[test]
    fn test_rtpc_compatible_value_static() {
        let value = RtpcCompatibleValue::static_value(0.5);
        assert_eq!(value.as_static(), Some(0.5));

        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"kind\":\"Static\""));
        assert!(json.contains("\"value\":0.5"));
    }

    #[test]
    fn test_rtpc_compatible_value_rtpc() {
        let curve = CurveDefinition {
            parts: vec![CurvePart {
                start: CurvePoint { x: 0.0, y: 1.0 },
                end: CurvePoint { x: 100.0, y: 0.0 },
                fader: FaderAlgorithm::Linear,
            }],
        };
        let value = RtpcCompatibleValue::rtpc(19, curve);
        assert_eq!(value.as_static(), None);

        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"kind\":\"RTPC\""));
        assert!(json.contains("\"id\":19"));
    }

    #[test]
    fn test_sound_loop_config() {
        let disabled = SoundLoopConfig::disabled();
        assert!(!disabled.enabled);
        assert_eq!(disabled.loop_count, 0);

        let infinite = SoundLoopConfig::infinite();
        assert!(infinite.enabled);
        assert_eq!(infinite.loop_count, 0);

        let finite = SoundLoopConfig::count(5);
        assert!(finite.enabled);
        assert_eq!(finite.loop_count, 5);
    }
}
