use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HeadgearSlotsFile {
    pub headgear: Vec<HeadgearSlotEntry>,
}

#[derive(Serialize)]
pub struct HeadgearSlotEntry {
    pub view: u32,
    pub slot: String,
    pub accname: String,
    pub items: Vec<u32>,
}

// ---------------------------------------------------------------------------
// accname_eng.lub parser
// ---------------------------------------------------------------------------

/// Extract the ordered accname list from the compiled Lua bytecode.
///
/// The bytecode contains null-terminated ASCII strings in order; we scan for
/// those matching `_[A-Z][A-Z0-9_]+` (the accname pattern), strip the leading
/// `_`, and lowercase.  The resulting Vec is 0-indexed; view_id 1 → index 0.
pub fn parse_accname_lub(data: &[u8]) -> Vec<String> {
    let mut results: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut buf: Vec<u8> = Vec::new();

    for &b in data {
        if b == 0 {
            if !buf.is_empty() {
                if let Ok(s) = std::str::from_utf8(&buf)
                    && is_accname(s) && !seen.contains(s) {
                        seen.insert(s.to_string());
                        results.push(s[1..].to_lowercase());
                    }
                buf.clear();
            }
        } else if (0x20..0x7f).contains(&b) {
            buf.push(b);
        } else {
            buf.clear();
        }
    }

    results
}

/// Returns true if the string matches `_[A-Z][A-Z0-9_]+`.
fn is_accname(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some('_') => {}
        _ => return false,
    }
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------------------
// rAthena headgear parser
// ---------------------------------------------------------------------------

/// Per-item headgear metadata extracted from rAthena item_db_equip.yml.
struct HeadgearItem {
    id: u32,
    view: u32,
    slot: HeadSlot,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum HeadSlot {
    Top,
    Mid,
    Low,
    Mixed, // item spans multiple head slots; default to Top
}

impl HeadSlot {
    fn as_str(self) -> &'static str {
        match self {
            HeadSlot::Top | HeadSlot::Mixed => "Head_Top",
            HeadSlot::Mid => "Head_Mid",
            HeadSlot::Low => "Head_Low",
        }
    }

    fn merge(self, other: HeadSlot) -> HeadSlot {
        if self == other { self } else { HeadSlot::Mixed }
    }
}

/// Parse rAthena `item_db_equip.yml` for headgear items.
/// Returns a map of view_id → list of (item_id, slot).
pub fn parse_headgear_items(path: &Path) -> HashMap<u32, Vec<(u32, HeadSlot)>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };

    let mut items: Vec<HeadgearItem> = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current_view: Option<u32> = None;
    let mut current_slot: Option<HeadSlot> = None;
    let mut in_location = false;

    for line in text.lines() {
        let s = line.trim();

        if let Some(rest) = s.strip_prefix("- Id:") {
            // Flush previous item.
            if let (Some(id), Some(view), Some(slot)) = (current_id, current_view, current_slot) {
                items.push(HeadgearItem { id, view, slot });
            }
            current_id = rest.split('#').next().unwrap_or("").trim().parse().ok();
            current_view = None;
            current_slot = None;
            in_location = false;
        } else if s == "Locations:" {
            in_location = true;
        } else if in_location {
            if s.starts_with("Head_Top:") {
                current_slot = Some(match current_slot {
                    Some(existing) => existing.merge(HeadSlot::Top),
                    None => HeadSlot::Top,
                });
            } else if s.starts_with("Head_Mid:") {
                current_slot = Some(match current_slot {
                    Some(existing) => existing.merge(HeadSlot::Mid),
                    None => HeadSlot::Mid,
                });
            } else if s.starts_with("Head_Low:") {
                current_slot = Some(match current_slot {
                    Some(existing) => existing.merge(HeadSlot::Low),
                    None => HeadSlot::Low,
                });
            } else if !s.is_empty() && !s.starts_with('#') {
                // Left the Location block.
                in_location = false;
            }
        }

        if let Some(rest) = s.strip_prefix("View:") {
            current_view = rest.split('#').next().unwrap_or("").trim().parse().ok();
        }
    }

    // Flush last item.
    if let (Some(id), Some(view), Some(slot)) = (current_id, current_view, current_slot) {
        items.push(HeadgearItem { id, view, slot });
    }

    // Group by view_id, filtering out view 0 (no sprite).
    let mut map: HashMap<u32, Vec<(u32, HeadSlot)>> = HashMap::new();
    for item in items {
        if item.view > 0 {
            map.entry(item.view).or_default().push((item.id, item.slot));
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Build the headgear slots list by joining the accname table with the
/// rAthena headgear item map.
///
/// Only view IDs that appear in `headgear_map` are emitted.  View IDs present
/// in `accnames` but absent from `headgear_map` are skipped (no item DB
/// entry, likely unused or unknown content).
pub fn build_headgear_slots(
    accnames: &[String],
    headgear_map: &HashMap<u32, Vec<(u32, HeadSlot)>>,
) -> Vec<HeadgearSlotEntry> {
    // Use a BTreeMap so output is sorted by view_id.
    let mut by_view: BTreeMap<u32, HeadgearSlotEntry> = BTreeMap::new();

    for (zero_idx, accname) in accnames.iter().enumerate() {
        let view_id = (zero_idx + 1) as u32;
        let Some(item_list) = headgear_map.get(&view_id) else {
            continue;
        };

        // Determine the canonical slot: if all items agree, use that slot;
        // otherwise fall back to Head_Top.
        let slot = item_list
            .iter()
            .map(|(_, s)| *s)
            .reduce(|a, b| a.merge(b))
            .unwrap_or(HeadSlot::Top)
            .as_str()
            .to_string();

        let mut item_ids: Vec<u32> = item_list.iter().map(|(id, _)| *id).collect();
        item_ids.sort_unstable();

        by_view.insert(
            view_id,
            HeadgearSlotEntry {
                view: view_id,
                slot,
                accname: accname.clone(),
                items: item_ids,
            },
        );
    }

    by_view.into_values().collect()
}

/// Serialize and write the headgear slots file.
pub fn write_headgear_slots(entries: Vec<HeadgearSlotEntry>, path: &Path) -> Result<()> {
    let file = HeadgearSlotsFile { headgear: entries };
    let text = toml::to_string_pretty(&file)?;
    std::fs::write(path, text)?;
    Ok(())
}
