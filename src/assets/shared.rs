//! Shared SDK types used across multiple asset types.
//!
//! This module contains types that map to shared FlatBuffer schemas in the
//! Amplitude Audio SDK (`common.fbs`, `sound_definition.fbs`, `collection_definition.fbs`).
//! These types are used by Sound, Collection, Effect, SwitchContainer, and Event assets.

use serde::{Deserialize, Serialize};

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

/// Reference to an RTPC (Real-Time Parameter Control) object with curve mapping.
///
/// Links a parameter to a specific RTPC by ID and defines how the RTPC's
/// value is transformed using a curve before being applied to the parameter.
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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Serialization Roundtrip Tests
    // =========================================================================

    #[test]
    fn test_p0_spatialization_serde_roundtrip() {
        for variant in [
            Spatialization::None,
            Spatialization::Position,
            Spatialization::PositionOrientation,
            Spatialization::Hrtf,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: Spatialization = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn test_p0_scope_serde_roundtrip() {
        for variant in [Scope::World, Scope::Entity] {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: Scope = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant);
        }
    }

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
    fn test_p0_rtpc_compatible_value_static_serde_roundtrip() {
        let value = RtpcCompatibleValue::static_value(0.75);
        let json = serde_json::to_string(&value).unwrap();
        let parsed: RtpcCompatibleValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, value);
    }

    #[test]
    fn test_p0_rtpc_compatible_value_rtpc_serde_roundtrip() {
        let value = RtpcCompatibleValue::rtpc(
            42,
            CurveDefinition {
                parts: vec![CurvePart {
                    start: CurvePoint { x: 0.0, y: 1.0 },
                    end: CurvePoint { x: 100.0, y: 0.0 },
                    fader: FaderAlgorithm::Linear,
                }],
            },
        );
        let json = serde_json::to_string(&value).unwrap();
        let parsed: RtpcCompatibleValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, value);
    }

    #[test]
    fn test_p0_sound_loop_config_serde_roundtrip() {
        for config in [
            SoundLoopConfig::disabled(),
            SoundLoopConfig::infinite(),
            SoundLoopConfig::count(5),
        ] {
            let json = serde_json::to_string(&config).unwrap();
            let parsed: SoundLoopConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, config);
        }
    }

    #[test]
    fn test_p0_curve_definition_serde_roundtrip() {
        let curve = CurveDefinition {
            parts: vec![
                CurvePart {
                    start: CurvePoint { x: 0.0, y: 0.0 },
                    end: CurvePoint { x: 50.0, y: 0.5 },
                    fader: FaderAlgorithm::EaseIn,
                },
                CurvePart {
                    start: CurvePoint { x: 50.0, y: 0.5 },
                    end: CurvePoint { x: 100.0, y: 1.0 },
                    fader: FaderAlgorithm::EaseOut,
                },
            ],
        };
        let json = serde_json::to_string(&curve).unwrap();
        let parsed: CurveDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, curve);
    }

    // =========================================================================
    // Default Trait Tests
    // =========================================================================

    #[test]
    fn test_p0_spatialization_default() {
        assert_eq!(Spatialization::default(), Spatialization::None);
    }

    #[test]
    fn test_p0_scope_default() {
        assert_eq!(Scope::default(), Scope::World);
    }

    #[test]
    fn test_p0_fader_algorithm_default() {
        assert_eq!(FaderAlgorithm::default(), FaderAlgorithm::Linear);
    }

    #[test]
    fn test_p0_sound_loop_config_default() {
        let config = SoundLoopConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.loop_count, 0);
    }

    #[test]
    fn test_p0_rtpc_compatible_value_default() {
        let value = RtpcCompatibleValue::default();
        assert_eq!(value.as_static(), Some(1.0));
    }

    // =========================================================================
    // Constructor Tests
    // =========================================================================

    #[test]
    fn test_p0_rtpc_compatible_value_static_value_constructor() {
        let value = RtpcCompatibleValue::static_value(0.5);
        assert_eq!(value.as_static(), Some(0.5));
    }

    #[test]
    fn test_p0_rtpc_compatible_value_rtpc_constructor() {
        let curve = CurveDefinition {
            parts: vec![CurvePart {
                start: CurvePoint { x: 0.0, y: 1.0 },
                end: CurvePoint { x: 100.0, y: 0.0 },
                fader: FaderAlgorithm::Linear,
            }],
        };
        let value = RtpcCompatibleValue::rtpc(19, curve);
        assert_eq!(value.as_static(), None);
    }

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

    // =========================================================================
    // SDK Format Tests
    // =========================================================================

    #[test]
    fn test_p0_spatialization_hrtf_serializes_as_hrtf() {
        assert_eq!(
            serde_json::to_string(&Spatialization::Hrtf).unwrap(),
            "\"HRTF\""
        );
    }

    #[test]
    fn test_p0_rtpc_compatible_value_static_tag() {
        let value = RtpcCompatibleValue::static_value(0.5);
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"kind\":\"Static\""));
        assert!(json.contains("\"value\":0.5"));
    }

    #[test]
    fn test_p0_rtpc_compatible_value_rtpc_tag() {
        let curve = CurveDefinition {
            parts: vec![CurvePart {
                start: CurvePoint { x: 0.0, y: 1.0 },
                end: CurvePoint { x: 100.0, y: 0.0 },
                fader: FaderAlgorithm::Linear,
            }],
        };
        let value = RtpcCompatibleValue::rtpc(19, curve);
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"kind\":\"RTPC\""));
        assert!(json.contains("\"id\":19"));
    }

    // =========================================================================
    // Constructor Tests - Curve Types
    // =========================================================================

    #[test]
    fn test_p0_curve_point_construction() {
        let point = CurvePoint { x: 0.5, y: 0.75 };
        assert_eq!(point.x, 0.5);
        assert_eq!(point.y, 0.75);
    }

    #[test]
    fn test_p0_curve_part_construction() {
        let part = CurvePart {
            start: CurvePoint { x: 0.0, y: 0.0 },
            end: CurvePoint { x: 1.0, y: 1.0 },
            fader: FaderAlgorithm::Linear,
        };
        assert_eq!(part.start.x, 0.0);
        assert_eq!(part.start.y, 0.0);
        assert_eq!(part.end.x, 1.0);
        assert_eq!(part.end.y, 1.0);
        assert_eq!(part.fader, FaderAlgorithm::Linear);
    }

    #[test]
    fn test_p0_curve_definition_construction() {
        let curve = CurveDefinition {
            parts: vec![
                CurvePart {
                    start: CurvePoint { x: 0.0, y: 0.0 },
                    end: CurvePoint { x: 50.0, y: 0.5 },
                    fader: FaderAlgorithm::EaseIn,
                },
                CurvePart {
                    start: CurvePoint { x: 50.0, y: 0.5 },
                    end: CurvePoint { x: 100.0, y: 1.0 },
                    fader: FaderAlgorithm::EaseOut,
                },
            ],
        };
        assert_eq!(curve.parts.len(), 2);
        assert_eq!(curve.parts[0].fader, FaderAlgorithm::EaseIn);
        assert_eq!(curve.parts[1].fader, FaderAlgorithm::EaseOut);
    }

    #[test]
    fn test_p0_rtpc_reference_construction() {
        let reference = RtpcReference {
            id: 42,
            curve: CurveDefinition {
                parts: vec![CurvePart {
                    start: CurvePoint { x: 0.0, y: 1.0 },
                    end: CurvePoint { x: 100.0, y: 0.0 },
                    fader: FaderAlgorithm::Linear,
                }],
            },
        };
        assert_eq!(reference.id, 42);
        assert_eq!(reference.curve.parts.len(), 1);
    }

    // =========================================================================
    // Exact JSON Format Regression Tests
    // =========================================================================

    #[test]
    fn test_p0_rtpc_compatible_value_static_exact_json() {
        let value = RtpcCompatibleValue::static_value(0.5);
        let json: serde_json::Value = serde_json::to_value(&value).unwrap();
        assert_eq!(json["kind"], "Static");
        assert_eq!(json["value"], 0.5);
        assert_eq!(
            json.as_object().unwrap().len(),
            2,
            "Static variant should have exactly 2 fields (kind, value)"
        );
    }

    #[test]
    fn test_p0_rtpc_compatible_value_rtpc_exact_json() {
        let value = RtpcCompatibleValue::rtpc(
            19,
            CurveDefinition {
                parts: vec![CurvePart {
                    start: CurvePoint { x: 0.0, y: 1.0 },
                    end: CurvePoint { x: 100.0, y: 0.0 },
                    fader: FaderAlgorithm::Linear,
                }],
            },
        );
        let json: serde_json::Value = serde_json::to_value(&value).unwrap();
        assert_eq!(
            json["kind"], "RTPC",
            "RTPC variant must use uppercase 'RTPC' tag"
        );
        assert_eq!(json["rtpc"]["id"], 19);
        assert!(json["rtpc"]["curve"]["parts"].is_array());
    }

    #[test]
    fn test_p0_scope_exact_json_values() {
        assert_eq!(serde_json::to_string(&Scope::World).unwrap(), "\"World\"");
        assert_eq!(serde_json::to_string(&Scope::Entity).unwrap(), "\"Entity\"");
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
}
