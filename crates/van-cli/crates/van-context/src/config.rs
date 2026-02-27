use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Represents the `package.json` project configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VanConfig {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scripts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
}

impl VanConfig {
    pub fn new(name: &str) -> Self {
        let mut scripts = BTreeMap::new();
        scripts.insert("dev".into(), "van dev".into());
        scripts.insert("build".into(), "van build".into());

        Self {
            name: name.into(),
            version: "0.1.0".into(),
            scripts,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            registry: None,
        }
    }

    pub fn to_json_pretty(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
