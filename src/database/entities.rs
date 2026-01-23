use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Source type for templates - embedded in binary or custom user-registered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TemplateSource {
    /// Template bundled with the CLI binary.
    #[default]
    Embedded,
    /// User-registered custom template.
    Custom,
}

impl Display for TemplateSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateSource::Embedded => write!(f, "Embedded"),
            TemplateSource::Custom => write!(f, "Custom"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ProjectConfiguration {
    pub name: String,
    pub default_configuration: String,
    pub sources_dir: String,
    pub data_dir: String,
    pub build_dir: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct Project {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Template {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    pub name: String,
    pub path: String,
    /// Target game engine for this template (e.g., "generic", "o3de").
    #[serde(default)]
    pub engine: Option<String>,
    /// Human-readable description of the template.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether this is an embedded or custom template.
    #[serde(default)]
    pub source: TemplateSource,
}

impl ProjectConfiguration {
    pub fn to_project(&self, path: &str) -> Project {
        Project {
            id: None,
            name: self.name.clone(),
            path: path.to_string(),
            registered_at: None,
        }
    }
}

impl Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.id.is_some() {
            write!(f, "{} ({})", self.name, self.path)
        } else {
            write!(f, "{}", self.name)
        }
    }
}
