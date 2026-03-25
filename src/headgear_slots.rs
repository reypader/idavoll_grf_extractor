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
// rAthena headgear parser
// ---------------------------------------------------------------------------

struct HeadgearItem {
    id: u32,
    view: u32,
    slot: HeadSlot,
    aegis_name: String,
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

/// Parse rAthena `item_db_equip.yml` for headgear items (View > 0, head location).
///
/// Returns a map of view_id → list of (item_id, slot, aegis_name).
/// The accname for each view group is later derived from the AegisName of
/// the lowest-ID item in that group.
pub fn parse_headgear_items(path: &Path) -> HashMap<u32, Vec<(u32, HeadSlot, String)>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };

    let mut items: Vec<HeadgearItem> = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current_view: Option<u32> = None;
    let mut current_slot: Option<HeadSlot> = None;
    let mut current_aegis: Option<String> = None;
    let mut in_location = false;

    for line in text.lines() {
        let s = line.trim();

        if let Some(rest) = s.strip_prefix("- Id:") {
            // Flush previous item.
            let aegis = current_aegis.take();
            if let (Some(id), Some(view), Some(slot), Some(aegis)) =
                (current_id, current_view, current_slot, aegis)
            {
                items.push(HeadgearItem { id, view, slot, aegis_name: aegis });
            }
            current_id = rest.split('#').next().unwrap_or("").trim().parse().ok();
            current_view = None;
            current_slot = None;
            in_location = false;
        } else if let Some(rest) = s.strip_prefix("AegisName:") {
            if current_aegis.is_none() {
                let aegis = rest.trim().trim_matches('"').to_string();
                if !aegis.is_empty() {
                    current_aegis = Some(aegis);
                }
            }
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
                // Left the Locations block.
                in_location = false;
            }
        }

        if let Some(rest) = s.strip_prefix("View:") {
            current_view = rest.split('#').next().unwrap_or("").trim().parse().ok();
        }
    }

    // Flush last item.
    let aegis = current_aegis.take();
    if let (Some(id), Some(view), Some(slot), Some(aegis)) =
        (current_id, current_view, current_slot, aegis)
    {
        items.push(HeadgearItem { id, view, slot, aegis_name: aegis });
    }

    // Group by view_id, filtering out view 0 (no sprite).
    let mut map: HashMap<u32, Vec<(u32, HeadSlot, String)>> = HashMap::new();
    for item in items {
        if item.view > 0 {
            map.entry(item.view).or_default().push((item.id, item.slot, item.aegis_name));
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Build the headgear slots list from the rAthena headgear item map.
///
/// The accname for each view group is the AegisName (lowercased) of the item
/// with the lowest ID in that group — this is the original/canonical item for
/// that view, whose AegisName matches the sprite identifier.
pub fn build_headgear_slots(
    headgear_map: &HashMap<u32, Vec<(u32, HeadSlot, String)>>,
) -> Vec<HeadgearSlotEntry> {
    // Use a BTreeMap so output is sorted by view_id.
    let mut by_view: BTreeMap<u32, HeadgearSlotEntry> = BTreeMap::new();

    for (&view_id, item_list) in headgear_map {
        // Determine the canonical slot: if all items agree, use that slot;
        // otherwise fall back to Head_Top.
        let slot = item_list
            .iter()
            .map(|(_, s, _)| *s)
            .reduce(|a, b| a.merge(b))
            .unwrap_or(HeadSlot::Top)
            .as_str()
            .to_string();

        // Derive accname from the AegisName of the lowest-ID (primary) item.
        let accname = item_list
            .iter()
            .min_by_key(|(id, _, _)| id)
            .map(|(_, _, aegis)| aegis.to_lowercase())
            .unwrap_or_default();

        if accname.is_empty() {
            continue;
        }

        let mut item_ids: Vec<u32> = item_list.iter().map(|(id, _, _)| *id).collect();
        item_ids.sort_unstable();

        by_view.insert(
            view_id,
            HeadgearSlotEntry { view: view_id, slot, accname, items: item_ids },
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
