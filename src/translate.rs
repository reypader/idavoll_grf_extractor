use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// translations.toml types
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize)]
pub struct TranslationsFile {
    #[serde(default)]
    pub known: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Translator
// ---------------------------------------------------------------------------

pub struct Translator {
    known: HashMap<String, String>,
    /// rAthena-derived lookup: Korean resource name → AegisName (lowercase).
    rathena: HashMap<String, String>,
    misses: BTreeSet<String>,
}

impl Translator {
    pub fn new(known: HashMap<String, String>, rathena: HashMap<String, String>) -> Self {
        Self {
            known,
            rathena,
            misses: BTreeSet::new(),
        }
    }

    /// Translate a full GRF internal path (backslash-separated, CP949-decoded).
    /// Returns the translated path using forward slashes.
    pub fn translate_path(&mut self, grf_path: &str) -> String {
        grf_path
            .split('\\')
            .map(|seg| self.translate_segment(seg))
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Translate a single path segment (directory name or filename).
    ///
    /// Strategy:
    /// 1. Pure ASCII → keep as-is.
    /// 2. Whole segment in `known` → use mapped value.
    /// 3. Whole segment in `rathena` → use AegisName.
    /// 4. Split on `_`, apply steps 1-3 per token; log misses.
    fn translate_segment(&mut self, segment: &str) -> String {
        if segment.is_ascii() {
            return segment.to_string();
        }

        // Try whole segment.
        if let Some(english) = self.lookup(segment) {
            return english;
        }

        // Split extension (e.g. ".spr", ".act").
        let (base, ext) = split_ext(segment);

        // Try whole base without extension.
        if !ext.is_empty()
            && let Some(english) = self.lookup(base) {
                return format!("{english}{ext}");
            }

        // Token-by-token translation on `_` boundaries.
        let tokens: Vec<String> = base
            .split('_')
            .map(|token| {
                if token.is_ascii() {
                    return token.to_string();
                }
                if let Some(english) = self.lookup(token) {
                    return english;
                }
                // Miss: log and keep original.
                self.misses.insert(token.to_string());
                token.to_string()
            })
            .collect();

        format!("{}{}", tokens.join("_"), ext)
    }

    fn lookup(&self, key: &str) -> Option<String> {
        self.known
            .get(key)
            .or_else(|| self.rathena.get(key))
            .cloned()
    }

    /// Returns all Korean segments that could not be translated.
    pub fn misses(&self) -> &BTreeSet<String> {
        &self.misses
    }
}

// ---------------------------------------------------------------------------
// Miss log serialization
// ---------------------------------------------------------------------------

/// Serialize misses to a TOML snippet the user can fill in and merge into
/// translations.toml.
pub fn format_miss_log(misses: &BTreeSet<String>) -> String {
    if misses.is_empty() {
        return String::new();
    }

    let mut log = String::from(
        "# Translation misses — fill in the English values and move entries to translations.toml\n\n\
         [known]\n",
    );

    // Use BTreeMap so output is sorted.
    let map: BTreeMap<&str, &str> = misses.iter().map(|k| (k.as_str(), "")).collect();
    for k in map.keys() {
        log.push_str(&format!("{} = \"\"\n", toml_key(k)));
    }
    log
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn split_ext(name: &str) -> (&str, &str) {
    if let Some(dot) = name.rfind('.') {
        (&name[..dot], &name[dot..])
    } else {
        (name, "")
    }
}

/// Produce a TOML-safe key string (quote if it contains special characters).
fn toml_key(s: &str) -> String {
    // TOML bare keys allow ASCII alphanumeric, `-`, and `_` only.
    // Korean always needs quoting.
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
