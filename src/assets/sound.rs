//! Sound asset type.
//!
//! Represents a single audio source with playback configuration for the
//! Amplitude Audio SDK. Sounds are the fundamental building blocks of audio
//! in a project, referencing audio files in the data/ directory.

use std::path::PathBuf;

use super::generated::{
    RtpcCompatibleValue, Scope, SoundDefinition, SoundLoopConfig, Spatialization,
};
use super::{Asset, AssetType, FaderAlgorithm, ProjectContext, Schema, ValidationError};

// =============================================================================
// Sound Type Alias
// =============================================================================

/// Individual sound definition.
///
/// Type alias to the build-time generated `SoundDefinition` from SDK FlatBuffer schemas.
/// Represents a single audio source with playback configuration.
/// Sounds reference audio files in the project's `data/` directory.
///
/// # Example
///
/// ```
/// use am::assets::Sound;
///
/// let sound = Sound::builder(12345, "explosion")
///     .path("sfx/explosion_01.wav")
///     .gain(0.8)
///     .priority(200)
///     .build();
/// ```
pub type Sound = SoundDefinition;

impl Sound {
    /// Creates a builder for constructing a Sound with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> SoundBuilder {
        SoundBuilder::new(id, name)
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
                name: Some(name.into()),
                path: Some(String::new()),
                bus: 0,
                gain: Some(RtpcCompatibleValue::static_value(1.0)),
                priority: Some(RtpcCompatibleValue::static_value(128.0)),
                stream: false,
                loop_: Some(SoundLoopConfig::disabled()),
                spatialization: Spatialization::None,
                attenuation: 0,
                scope: Scope::World,
                fader: Some(FaderAlgorithm::Linear.to_string()),
                effect: 0,
                near_field_gain: None,
                pitch: None,
            },
        }
    }

    /// Sets the path to the audio file.
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        let path_buf: PathBuf = path.into();
        self.sound.path = path_buf.to_str().map(|s| s.to_string());
        self
    }

    /// Sets the bus ID.
    pub fn bus(mut self, bus_id: u64) -> Self {
        self.sound.bus = bus_id;
        self
    }

    /// Sets the gain as a static value.
    pub fn gain(mut self, value: f32) -> Self {
        self.sound.gain = Some(RtpcCompatibleValue::static_value(value));
        self
    }

    /// Sets the gain with full RtpcCompatibleValue control.
    pub fn gain_rtpc(mut self, value: RtpcCompatibleValue) -> Self {
        self.sound.gain = Some(value);
        self
    }

    /// Sets the priority as a static value.
    pub fn priority(mut self, value: u8) -> Self {
        self.sound.priority = Some(RtpcCompatibleValue::static_value(value as f32));
        self
    }

    /// Sets the priority with full RtpcCompatibleValue control.
    pub fn priority_rtpc(mut self, value: RtpcCompatibleValue) -> Self {
        self.sound.priority = Some(value);
        self
    }

    /// Enables streaming from disk.
    pub fn stream(mut self, stream: bool) -> Self {
        self.sound.stream = stream;
        self
    }

    /// Sets the loop configuration.
    pub fn loop_config(mut self, config: SoundLoopConfig) -> Self {
        self.sound.loop_ = Some(config);
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
        self.sound.fader = Some(fader.to_string());
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
        self.name.as_deref().unwrap_or("")
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
        // Validate name is not empty
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Sound asset has no name",
                "Sound assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"explosion\")")
            .with_field("name"));
        }

        // Check audio file path is set and exists
        let path_str = self.path.as_deref().unwrap_or("");
        if path_str.is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Sound asset has no audio file path",
                "Sound assets must reference an audio file in the data/ directory",
            )
            .with_suggestion(
                "Set the 'path' field to a valid audio file path (e.g., \"sfx/explosion.wav\")",
            )
            .with_field("path"));
        }
        let audio_path = context.project_root.join("data").join(path_str);
        if !audio_path.exists() {
            return Err(ValidationError::type_rule_violation(
                format!("Audio file not found: {}", path_str),
                "Sound assets must reference an existing audio file in the data/ directory",
            )
            .with_suggestion(format!("Add the audio file at: {}", audio_path.display()))
            .with_field("path"));
        }

        // Check gain range (if static value)
        if let Some(gain_value) = self.gain.as_ref().and_then(|g| g.as_static())
            && (!gain_value.is_finite() || !(0.0..=1.0).contains(&gain_value))
        {
            return Err(ValidationError::type_rule_violation(
                format!("Invalid gain value: {}", gain_value),
                "Gain must be between 0.0 and 1.0",
            )
            .with_suggestion("Set gain to a value between 0.0 (silent) and 1.0 (full volume)")
            .with_field("gain"));
        }

        // Validate fader algorithm is a known value
        if let Some(fader_str) = &self.fader {
            if FaderAlgorithm::from_str(fader_str).is_err() {
                return Err(ValidationError::type_rule_violation(
                    format!("Unknown fader algorithm: '{}'", fader_str),
                    "Fader must be a valid algorithm name",
                )
                .with_suggestion(format!(
                    "Use one of: {}",
                    super::extensions::FADER_ALGORITHM_NAMES.join(", ")
                ))
                .with_field("fader"));
            }
        }

        // Cross-asset reference checks (only if validator is available)
        // Bus and attenuation validation deferred to Epic 6.
        if let Some(validator) = &context.validator {
            // Validate effect reference (zero IDs handled internally as no-op)
            validator
                .validate_effect_exists(self.effect)
                .map_err(|e| e.with_field("effect"))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_builder_basic() {
        let sound = Sound::builder(12345, "explosion")
            .path("sfx/explosion.wav")
            .build();
        assert_eq!(sound.id(), 12345);
        assert_eq!(sound.name(), "explosion");
        assert_eq!(sound.path.as_deref(), Some("sfx/explosion.wav"));
    }

    #[test]
    fn test_sound_builder_defaults() {
        let sound = Sound::builder(1, "test").build();
        assert_eq!(sound.id, 1);
        assert_eq!(sound.name.as_deref(), Some("test"));
        assert_eq!(sound.path.as_deref(), Some(""));
        assert_eq!(sound.bus, 0);
        assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(1.0));
        assert_eq!(
            sound.priority.as_ref().and_then(|p| p.as_static()),
            Some(128.0)
        );
        assert!(!sound.stream);
        assert_eq!(sound.loop_.as_ref(), Some(&SoundLoopConfig::disabled()));
        assert_eq!(sound.spatialization, Spatialization::None);
        assert_eq!(sound.scope, Scope::World);
        assert_eq!(sound.fader.as_deref(), Some("Linear"));
    }

    #[test]
    fn test_sound_builder_all_fields() {
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
        assert_eq!(sound.name.as_deref(), Some("explosion"));
        assert_eq!(sound.path.as_deref(), Some("sfx/explosion.wav"));
        assert_eq!(sound.bus, 100);
        assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(0.8));
        assert_eq!(
            sound.priority.as_ref().and_then(|p| p.as_static()),
            Some(200.0)
        );
        assert!(sound.stream);
        assert_eq!(
            sound.loop_,
            Some(SoundLoopConfig {
                enabled: true,
                loop_count: 0
            })
        );
        assert_eq!(sound.spatialization, Spatialization::Position);
        assert_eq!(sound.attenuation, 50);
        assert_eq!(sound.scope, Scope::Entity);
        assert_eq!(sound.fader.as_deref(), Some("SCurveSmooth"));
        assert_eq!(sound.effect, 25);
    }

    #[test]
    fn test_sound_asset_type() {
        let sound = Sound::builder(1, "test").build();
        assert_eq!(sound.asset_type(), AssetType::Sound);
    }

    #[test]
    fn test_sound_file_extension() {
        let sound = Sound::builder(1, "test").build();
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
        assert_eq!(parsed.loop_, sound.loop_);
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
            .spatialization(Spatialization::HRTF)
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
        assert_eq!(sound.name.as_deref(), Some("footstep"));
        assert_eq!(sound.path.as_deref(), Some("sfx/footstep_01.wav"));
        assert_eq!(sound.bus, 100);
        assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(0.8));
        assert_eq!(
            sound.priority.as_ref().and_then(|p| p.as_static()),
            Some(128.0)
        );
        assert!(!sound.stream);
        assert_eq!(
            sound.loop_,
            Some(SoundLoopConfig {
                enabled: false,
                loop_count: 0
            })
        );
        assert_eq!(sound.spatialization, Spatialization::Position);
        assert_eq!(sound.scope, Scope::World);
        assert_eq!(sound.fader.as_deref(), Some("Linear"));
    }

    #[test]
    fn test_sound_validate_rules_passes_valid() {
        use std::fs;
        use tempfile::tempdir;

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
    fn test_sound_validate_rules_fails_empty_path() {
        let context = ProjectContext::empty();
        let sound = Sound::builder(1, "test").build(); // default path is empty string
        let result = sound.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no audio file path"));
        assert_eq!(err.field, Some("path".to_string()));
    }

    #[test]
    fn test_sound_validate_rules_fails_invalid_gain() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().join("data").join("sfx");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("test.wav"), b"fake audio").unwrap();
        let context = ProjectContext::new(temp_dir.path().to_path_buf());

        // Test gain > 1.0
        let sound = Sound::builder(1, "test")
            .path("sfx/test.wav")
            .gain(1.5)
            .build();

        let result = sound.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("Invalid gain value"));
        assert_eq!(err.field, Some("gain".to_string()));

        // Test gain < 0.0
        let sound = Sound::builder(2, "test2")
            .path("sfx/test.wav")
            .gain(-0.5)
            .build();

        let result = sound.validate_rules(&context);
        assert!(result.is_err());
    }

    #[test]
    fn test_sound_validate_rules_fails_nan_gain() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().join("data").join("sfx");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("test.wav"), b"fake audio").unwrap();
        let context = ProjectContext::new(temp_dir.path().to_path_buf());

        let sound = Sound::builder(1, "test")
            .path("sfx/test.wav")
            .gain(f32::NAN)
            .build();

        let result = sound.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Invalid gain value"));
    }

    #[test]
    fn test_sound_validate_rules_fails_invalid_fader() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().join("data").join("sfx");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("test.wav"), b"fake audio").unwrap();
        let context = ProjectContext::new(temp_dir.path().to_path_buf());

        let mut sound = Sound::builder(1, "test").path("sfx/test.wav").build();
        sound.fader = Some("InvalidFader".to_string());

        let result = sound.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Unknown fader algorithm"));
        assert_eq!(err.field, Some("fader".to_string()));
    }
}
