use std::collections::HashMap;
use std::path::Path;

/// Parse `idnum2itemresnametable.txt` (CP949, extracted from GRF as raw bytes).
/// Format per line: `ID#KoreanResName#` or `ID#KoreanResName#AltName#`
/// Returns `HashMap<u32, String>` mapping item ID → Korean resource name.
pub fn parse_item_res_table(data: &[u8]) -> HashMap<u32, String> {
    let mut map = HashMap::new();

    let (text, _, _) = encoding_rs::EUC_KR.decode(data);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let mut parts = line.split('#');
        let id_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let res_name = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        if res_name.is_empty() {
            continue;
        }
        if let Ok(id) = id_str.parse::<u32>() {
            map.insert(id, res_name.to_string());
        }
    }

    map
}

/// Parse rAthena `item_db_equip.yml` (and similar YAML files) into an
/// `HashMap<u32, String>` mapping item ID → AegisName.
///
/// Uses a simple line-by-line parser rather than a full YAML library to
/// avoid an extra dependency; the format is regular enough.
pub fn parse_rathena_item_db(path: &Path) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };

    let mut current_id: Option<u32> = None;
    for line in text.lines() {
        let s = line.trim();
        if let Some(rest) = s.strip_prefix("- Id:") {
            let id_str = rest.split('#').next().unwrap_or("").trim();
            current_id = id_str.parse::<u32>().ok();
        } else if let Some(rest) = s.strip_prefix("AegisName:")
            && let Some(id) = current_id {
                let aegis = rest.trim().trim_matches('"').to_string();
                if !aegis.is_empty() {
                    map.insert(id, aegis);
                }
                current_id = None; // only capture the first AegisName per entry
            }
    }

    map
}

/// Build a lookup from Korean resource name → AegisName by joining the
/// GRF item res table with the rAthena item DB.
pub fn build_res_to_aegis(
    res_table: &HashMap<u32, String>,
    rathena_dbs: &[HashMap<u32, String>],
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (id, korean_res) in res_table {
        for db in rathena_dbs {
            if let Some(aegis) = db.get(id) {
                // Convert AegisName to lowercase with underscores for
                // consistency with the rest of the translated output.
                map.insert(korean_res.clone(), aegis.to_lowercase());
                break;
            }
        }
    }
    map
}
