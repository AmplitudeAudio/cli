//! Hand-written types and extension methods for generated SDK types.
//!
//! Contains:
//! - `FaderAlgorithm` enum (no generated equivalent — SDK stores fader as a string)
//! - Convenience methods on generated `RtpcCompatibleValue` and `SoundLoopConfig`

use serde::{Deserialize, Serialize};

use super::generated::{RtpcCompatibleValue, SoundLoopConfig, ValueKind};

// =============================================================================
// FaderAlgorithm Enum
// =============================================================================

/// Algorithm used for fade-in/fade-out animations.
///
/// The SDK stores fader as a string field on generated structs (e.g., `SoundDefinition.fader`).
/// This enum provides type safety for the builder API and can be converted to/from `String`.
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

impl std::fmt::Display for FaderAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Constant => write!(f, "Constant"),
            Self::Ease => write!(f, "Ease"),
            Self::EaseIn => write!(f, "EaseIn"),
            Self::EaseInOut => write!(f, "EaseInOut"),
            Self::EaseOut => write!(f, "EaseOut"),
            Self::Linear => write!(f, "Linear"),
            Self::SCurveSmooth => write!(f, "SCurveSmooth"),
            Self::SCurveSharp => write!(f, "SCurveSharp"),
        }
    }
}

/// All valid fader algorithm names, for use in error messages.
pub const FADER_ALGORITHM_NAMES: &[&str] = &[
    "Constant",
    "Ease",
    "EaseIn",
    "EaseInOut",
    "EaseOut",
    "Linear",
    "SCurveSmooth",
    "SCurveSharp",
];

impl FaderAlgorithm {
    /// Parse a fader algorithm from its string representation.
    ///
    /// Returns an error with context if the string doesn't match any known algorithm.
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "Constant" => Ok(Self::Constant),
            "Ease" => Ok(Self::Ease),
            "EaseIn" => Ok(Self::EaseIn),
            "EaseInOut" => Ok(Self::EaseInOut),
            "EaseOut" => Ok(Self::EaseOut),
            "Linear" => Ok(Self::Linear),
            "SCurveSmooth" => Ok(Self::SCurveSmooth),
            "SCurveSharp" => Ok(Self::SCurveSharp),
            _ => Err(format!(
                "Unknown fader algorithm '{}'. Valid values: {}",
                s,
                FADER_ALGORITHM_NAMES.join(", ")
            )),
        }
    }
}

// =============================================================================
// RtpcCompatibleValue Convenience Methods
// =============================================================================

impl RtpcCompatibleValue {
    /// Creates a static value.
    pub fn static_value(value: f32) -> Self {
        Self {
            kind: ValueKind::Static,
            value,
            rtpc: None,
        }
    }

    /// Creates an RTPC-controlled value.
    pub fn rtpc(id: u64, curve: super::generated::CurveDefinition) -> Self {
        Self {
            kind: ValueKind::RTPC,
            value: 0.0,
            rtpc: Some(super::generated::RtpcParameter {
                id,
                curve: Some(curve),
            }),
        }
    }

    /// Returns the static value if this is a Static variant.
    pub fn as_static(&self) -> Option<f32> {
        if self.kind == ValueKind::Static {
            Some(self.value)
        } else {
            None
        }
    }
}

impl Default for RtpcCompatibleValue {
    fn default() -> Self {
        Self::static_value(1.0)
    }
}

// =============================================================================
// SoundLoopConfig Convenience Methods
// =============================================================================

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

impl Default for SoundLoopConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

impl Eq for SoundLoopConfig {}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // FaderAlgorithm Tests
    // =========================================================================

    #[test]
    fn test_p0_fader_algorithm_serde_roundtrip() {
        for variant in [
            FaderAlgorithm::Constant,
            FaderAlgorithm::Ease,
            FaderAlgorithm::EaseIn,
            FaderAlgorithm::EaseInOut,
            FaderAlgorithm::EaseOut,
            FaderAlgorithm::Linear,
            FaderAlgorithm::SCurveSmooth,
            FaderAlgorithm::SCurveSharp,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: FaderAlgorithm = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn test_p0_fader_algorithm_default() {
        assert_eq!(FaderAlgorithm::default(), FaderAlgorithm::Linear);
    }

    #[test]
    fn test_p0_fader_algorithm_display() {
        assert_eq!(FaderAlgorithm::Linear.to_string(), "Linear");
        assert_eq!(FaderAlgorithm::SCurveSmooth.to_string(), "SCurveSmooth");
        assert_eq!(FaderAlgorithm::SCurveSharp.to_string(), "SCurveSharp");
    }

    #[test]
    fn test_p0_fader_algorithm_from_str() {
        assert_eq!(
            FaderAlgorithm::from_str("Linear"),
            Ok(FaderAlgorithm::Linear)
        );
        assert_eq!(
            FaderAlgorithm::from_str("SCurveSmooth"),
            Ok(FaderAlgorithm::SCurveSmooth)
        );
        let err = FaderAlgorithm::from_str("Unknown").unwrap_err();
        assert!(err.contains("Unknown fader algorithm 'Unknown'"));
        assert!(err.contains("Linear"));
    }

    #[test]
    fn test_p0_fader_algorithm_exact_json_values() {
        assert_eq!(
            serde_json::to_string(&FaderAlgorithm::SCurveSmooth).unwrap(),
            "\"SCurveSmooth\""
        );
        assert_eq!(
            serde_json::to_string(&FaderAlgorithm::SCurveSharp).unwrap(),
            "\"SCurveSharp\""
        );
    }

    // =========================================================================
    // RtpcCompatibleValue Convenience Method Tests
    // =========================================================================

    #[test]
    fn test_p0_rtpc_compatible_value_static_value_constructor() {
        let value = RtpcCompatibleValue::static_value(0.5);
        assert_eq!(value.as_static(), Some(0.5));
        assert_eq!(value.kind, ValueKind::Static);
        assert!(value.rtpc.is_none());
    }

    #[test]
    fn test_p0_rtpc_compatible_value_default() {
        let value = RtpcCompatibleValue::default();
        assert_eq!(value.as_static(), Some(1.0));
    }

    #[test]
    fn test_p0_rtpc_compatible_value_as_static_returns_none_for_rtpc() {
        let value = RtpcCompatibleValue {
            kind: ValueKind::RTPC,
            value: 0.0,
            rtpc: None,
        };
        assert_eq!(value.as_static(), None);
    }

    // =========================================================================
    // SoundLoopConfig Convenience Method Tests
    // =========================================================================

    #[test]
    fn test_p0_sound_loop_config_disabled() {
        let config = SoundLoopConfig::disabled();
        assert!(!config.enabled);
        assert_eq!(config.loop_count, 0);
    }

    #[test]
    fn test_p0_sound_loop_config_infinite() {
        let config = SoundLoopConfig::infinite();
        assert!(config.enabled);
        assert_eq!(config.loop_count, 0);
    }

    #[test]
    fn test_p0_sound_loop_config_count() {
        let config = SoundLoopConfig::count(5);
        assert!(config.enabled);
        assert_eq!(config.loop_count, 5);
    }

    #[test]
    fn test_p0_sound_loop_config_default() {
        let config = SoundLoopConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.loop_count, 0);
    }
}
