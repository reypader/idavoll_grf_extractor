use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct WeaponTypesFile {
    pub weapon_type: Vec<WeaponTypeEntry>,
}

#[derive(Serialize)]
pub struct WeaponTypeEntry {
    /// Numeric weapon type ID used internally by Ragnarok Online.
    pub id: u32,
    /// Sprite directory name for this weapon type (matches the translated GRF path segment).
    pub name: String,
    /// All item IDs of this weapon type (sorted).
    pub items: Vec<u32>,
}

// ---------------------------------------------------------------------------
// SubType → (type_id, sprite_name) mapping
// ---------------------------------------------------------------------------

/// Map an rAthena weapon SubType string to `(weapon_type_id, sprite_dir_name)`.
///
/// SubType names follow rAthena's YAML convention (e.g. `1hSword`, `2hSword`).
/// IDs follow the standard Ragnarok Online weapon type table as defined in
/// rAthena's `src/map/pc.hpp` (W_DAGGER=1 … W_2HSTAFF=23).
fn subtype_to_weapon_type(subtype: &str) -> Option<(u32, &'static str)> {
    match subtype {
        "Dagger" => Some((1, "dagger")),
        "1hSword" => Some((2, "sword")),
        "2hSword" => Some((3, "two_handed_sword")),
        "1hSpear" => Some((4, "spear")),
        "2hSpear" => Some((5, "two_handed_spear")),
        "1hAxe" => Some((6, "axe")),
        "2hAxe" => Some((7, "two_handed_axe")),
        "Mace" => Some((8, "mace")),
        "Staff" => Some((10, "staff")),
        "Bow" => Some((11, "bow")),
        "Knuckle" => Some((12, "knuckle")),
        "Musical" => Some((13, "musical")),
        "Whip" => Some((14, "whip")),
        "Book" => Some((15, "book")),
        "Katar" => Some((16, "katar")),
        "Revolver" => Some((17, "revolver")),
        "Rifle" => Some((18, "rifle")),
        "Gatling" => Some((19, "gatling_gun")),
        "Shotgun" => Some((20, "shotgun")),
        "Grenade" => Some((21, "grenade_launcher")),
        "Huuma" => Some((22, "fuuma_shuriken")),
        "2hStaff" => Some((23, "two_handed_staff")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// rAthena parser
// ---------------------------------------------------------------------------

/// Parse rAthena `item_db_equip.yml` for weapon items (`Type: Weapon`).
///
/// Returns a map of SubType string → list of item IDs.
pub fn parse_weapon_items(path: &Path) -> BTreeMap<String, Vec<u32>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };

    let mut result: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut current_id: Option<u32> = None;
    let mut current_is_weapon = false;
    let mut current_subtype: Option<String> = None;

    for line in text.lines() {
        let s = line.trim();

        if let Some(rest) = s.strip_prefix("- Id:") {
            // Flush previous item.
            let subtype = current_subtype.take();
            if current_is_weapon
                && let (Some(id), Some(subtype)) = (current_id, subtype)
                && subtype_to_weapon_type(&subtype).is_some()
            {
                result.entry(subtype).or_default().push(id);
            }
            current_id = rest.split('#').next().unwrap_or("").trim().parse().ok();
            current_is_weapon = false;
        } else if s == "Type: Weapon" {
            current_is_weapon = true;
        } else if let Some(rest) = s.strip_prefix("SubType:")
            && current_subtype.is_none()
        {
            let st = rest.trim().trim_matches('"').to_string();
            if !st.is_empty() {
                current_subtype = Some(st);
            }
        }
    }

    // Flush last item.
    if current_is_weapon
        && let (Some(id), Some(subtype)) = (current_id, current_subtype)
        && subtype_to_weapon_type(&subtype).is_some()
    {
        result.entry(subtype).or_default().push(id);
    }

    result
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Build the weapon types list from the parsed SubType → item IDs map.
/// Output is sorted by weapon type ID.
pub fn build_weapon_types(weapon_map: BTreeMap<String, Vec<u32>>) -> Vec<WeaponTypeEntry> {
    let mut by_id: BTreeMap<u32, WeaponTypeEntry> = BTreeMap::new();

    for (subtype, mut item_ids) in weapon_map {
        let Some((type_id, name)) = subtype_to_weapon_type(&subtype) else {
            continue;
        };
        item_ids.sort_unstable();
        by_id.insert(type_id, WeaponTypeEntry { id: type_id, name: name.to_string(), items: item_ids });
    }

    by_id.into_values().collect()
}

/// Serialize and write the weapon types file.
pub fn write_weapon_types(entries: Vec<WeaponTypeEntry>, path: &Path) -> Result<()> {
    let file = WeaponTypesFile { weapon_type: entries };
    let text = toml::to_string_pretty(&file)?;
    std::fs::write(path, text)?;
    Ok(())
}
