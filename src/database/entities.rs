use serde::{Deserialize, Serialize};
use std::fmt::Display;

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
    pub id: Option<i32>,
    pub name: String,
    pub path: String,
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
