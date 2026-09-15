//! Personas: named system prompts (plus optional model/variant defaults)
//! managed through the settings UI and stored in the config file.

use serde::{Deserialize, Serialize};

use crate::config::ModelRef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Persona {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub model_ref: Option<ModelRef>,
    pub variant: Option<String>,
}

impl Persona {
    pub fn new(name: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            system_prompt: system_prompt.into(),
            model_ref: None,
            variant: None,
        }
    }
}
