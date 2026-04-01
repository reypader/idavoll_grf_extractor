use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

fn default_true() -> bool {
    true
}

/// A named extraction bundle: matches GRF entries by path prefix or file extension.
#[derive(Deserialize)]
pub struct Bundle {
    pub name: String,
    /// Translated path prefixes — any file whose path starts with one of these is included.
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    /// File extensions (without leading dot) — any file with a matching extension is included.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Whether to apply Korean→English translation for paths in this bundle (default: true).
    #[serde(default = "default_true")]
    pub translate: bool,
}

#[derive(Deserialize)]
pub struct BundlesFile {
    pub bundle: Vec<Bundle>,
}

pub fn load(path: &Path) -> Result<BundlesFile> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Returns true if `translated_path` matches any rule in `bundle`.
pub fn matches(translated_path: &str, bundle: &Bundle) -> bool {
    for prefix in &bundle.path_prefixes {
        if translated_path.starts_with(prefix.as_str()) {
            return true;
        }
    }
    let lower = translated_path.to_ascii_lowercase();
    for ext in &bundle.extensions {
        let suffix = format!(".{}", ext.to_ascii_lowercase());
        if lower.ends_with(&suffix) {
            return true;
        }
    }
    false
}

/// Returns true if `translated_path` matches any rule in any of the given bundles.
pub fn matches_any(translated_path: &str, bundles: &[&Bundle]) -> bool {
    bundles.iter().any(|b| matches(translated_path, b))
}
