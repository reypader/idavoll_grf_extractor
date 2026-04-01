mod bundles;
mod decrypt;
mod grf;
mod headgear_slots;
mod rathena;
mod translate;
mod weapon_types;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use grf::Grf;
use translate::{format_miss_log, Translator, TranslationsFile};

const ITEM_RES_TABLE_PATH: &str = "data\\idnum2itemresnametable.txt";

#[derive(Parser)]
#[command(name = "grf-extract")]
#[command(about = "Extract a Ragnarok Online .grf archive with Korean→English filename translation")]
struct Args {
    /// Path to the .grf file
    grf: PathBuf,

    /// Output directory
    #[arg(short, long, default_value = "extracted")]
    output: PathBuf,

    /// Path to translations.toml
    #[arg(short, long, default_value = "translations.toml")]
    translations: PathBuf,

    /// Path to rAthena db/ directory for item ID → AegisName resolution
    #[arg(long, value_name = "PATH")]
    rathena_db: Option<PathBuf>,

    /// Where to write the generated headgear slots file (requires --rathena-db)
    #[arg(long, default_value = "headgear_slots.toml")]
    headgear_slots: PathBuf,

    /// Where to write the generated weapon types file (requires --rathena-db)
    #[arg(long, default_value = "weapon_types.toml")]
    weapon_types: PathBuf,

    /// Where to write the miss log (untranslated Korean segments)
    #[arg(long, default_value = "miss_log.toml")]
    miss_log: PathBuf,

    /// Parse and translate paths without writing any files (still writes miss log)
    #[arg(long)]
    dry_run: bool,

    /// Print each extracted file path
    #[arg(short, long)]
    verbose: bool,

    /// Path to bundles.toml defining extraction bundles
    #[arg(long, default_value = "bundles.toml")]
    bundles: PathBuf,

    /// Only extract files matching these bundle names, comma-separated (omit to extract everything)
    #[arg(long, value_delimiter = ',')]
    extract: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load hand-curated translation dictionary.
    let known = load_known(&args.translations)?;

    // Open GRF.
    let file = fs::File::open(&args.grf)
        .with_context(|| format!("opening {}", args.grf.display()))?;
    let mut grf = Grf::open(file)
        .with_context(|| format!("parsing {}", args.grf.display()))?;

    println!("GRF: {} entries", grf.entries.len());

    // Build rAthena lookup: Korean res name → AegisName.
    let rathena_lookup = build_rathena_lookup(&mut grf, args.rathena_db.as_deref())?;
    if !rathena_lookup.is_empty() {
        println!("rAthena: {} item res name mappings loaded", rathena_lookup.len());
    }

    // Generate headgear_slots.toml and weapon_types.toml when rAthena DB is available.
    if let Some(ref db_path) = args.rathena_db {
        generate_headgear_slots(db_path, &args.headgear_slots)?;
        generate_weapon_types(db_path, &args.weapon_types)?;
    }

    // Always load bundles.toml — needed for translate=false passthrough as well as
    // the --extract bundle filter.
    let bundles_file: bundles::BundlesFile = bundles::load(&args.bundles)?;

    // Validate --extract names.
    if !args.extract.is_empty() {
        for name in &args.extract {
            if !bundles_file.bundle.iter().any(|b| b.name == *name) {
                let known: Vec<&str> = bundles_file.bundle.iter().map(|b| b.name.as_str()).collect();
                eprintln!("WARN: unknown bundle '{name}'; known bundles: {}", known.join(", "));
            }
        }
    }

    // Collect path prefixes from bundles where translate = false. Paths that
    // start with any of these are forwarded as-is (backslash → slash only),
    // bypassing Korean→English translation.
    let passthrough_prefixes: Vec<&str> = bundles_file
        .bundle
        .iter()
        .filter(|b| !b.translate)
        .flat_map(|b| b.path_prefixes.iter().map(|s| s.as_str()))
        .collect();

    // Active bundles for the --extract filter (None = extract everything).
    let active_bundles: Option<Vec<&bundles::Bundle>> = if args.extract.is_empty() {
        None
    } else {
        Some(
            bundles_file
                .bundle
                .iter()
                .filter(|b| args.extract.iter().any(|n| n == &b.name))
                .collect(),
        )
    };

    if let Some(ref active) = active_bundles {
        let names: Vec<&str> = active.iter().map(|b| b.name.as_str()).collect();
        println!("Bundle filter: {}", names.join(", "));
    }

    // Translate all paths up front.
    let translated_paths: Vec<String> = {
        let mut t = Translator::new(known, rathena_lookup);
        let paths: Vec<String> = grf
            .entries
            .iter()
            .map(|e| {
                let normalized = e.internal_path.replace('\\', "/");
                if passthrough_prefixes.iter().any(|p| normalized.starts_with(p)) {
                    normalized
                } else {
                    t.translate_path(&e.internal_path)
                }
            })
            .collect();

        // Write miss log.
        let miss_log = format_miss_log(t.misses());
        if !miss_log.is_empty() {
            fs::write(&args.miss_log, &miss_log)
                .with_context(|| format!("writing miss log {}", args.miss_log.display()))?;
            println!(
                "Miss log: {} ({} untranslated segments)",
                args.miss_log.display(),
                t.misses().len()
            );
        }

        paths
    };

    // Extract files.
    let mut extracted = 0usize;
    let mut skipped = 0usize;

    // Snapshot entry metadata so we can borrow grf mutably for read_entry.
    let entry_meta: Vec<(String, u32, u32, u32, u8, u64)> = grf
        .entries
        .iter()
        .map(|e| {
            (
                e.internal_path.clone(),
                e.pack_size,
                e.length_aligned,
                e.real_size,
                e.entry_type,
                e.data_offset,
            )
        })
        .collect();

    for (i, (internal_path, pack_size, length_aligned, real_size, entry_type, data_offset)) in
        entry_meta.into_iter().enumerate()
    {
        let out_path = args.output.join(&translated_paths[i]);

        if let Some(ref active) = active_bundles
            && !bundles::matches_any(&translated_paths[i], active) {
                continue;
            }

        if args.verbose {
            println!("{internal_path} -> {}", translated_paths[i]);
        }

        if args.dry_run {
            extracted += 1;
            continue;
        }

        // Create parent directory.
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }

        let entry = grf::GrfEntry {
            internal_path: internal_path.clone(),
            pack_size,
            length_aligned,
            real_size,
            entry_type,
            data_offset,
        };

        let data = match grf.read_entry(&entry) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("WARN: skipping {internal_path} — {e}");
                skipped += 1;
                continue;
            }
        };

        fs::write(&out_path, &data)
            .with_context(|| format!("writing {}", out_path.display()))?;

        extracted += 1;
    }

    println!("Extracted: {extracted}  Skipped: {skipped}");

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_known(path: &Path) -> Result<HashMap<String, String>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading translations file {}", path.display()))?;
    let file: TranslationsFile = toml::from_str(&text)
        .with_context(|| format!("parsing translations file {}", path.display()))?;
    Ok(file.known)
}

/// Parse rAthena item DB to generate headgear_slots.toml.
///
/// Accnames are derived from the AegisName of the lowest-ID item per view group,
/// eliminating the need to scan accname_eng.lub from the GRF.
fn generate_headgear_slots(rathena_db: &Path, out_path: &Path) -> Result<()> {
    let equip_db = rathena_db.join("re/item_db_equip.yml");
    let headgear_map = headgear_slots::parse_headgear_items(&equip_db);
    let entries = headgear_slots::build_headgear_slots(&headgear_map);
    let count = entries.len();
    headgear_slots::write_headgear_slots(entries, out_path)
        .with_context(|| format!("writing {}", out_path.display()))?;
    println!("Headgear slots: {} entries → {}", count, out_path.display());
    Ok(())
}

/// Parse rAthena item DB to generate weapon_types.toml.
fn generate_weapon_types(rathena_db: &Path, out_path: &Path) -> Result<()> {
    let equip_db = rathena_db.join("re/item_db_equip.yml");
    let weapon_map = weapon_types::parse_weapon_items(&equip_db);
    let entries = weapon_types::build_weapon_types(weapon_map);
    let count = entries.len();
    weapon_types::write_weapon_types(entries, out_path)
        .with_context(|| format!("writing {}", out_path.display()))?;
    println!("Weapon types: {} entries → {}", count, out_path.display());
    Ok(())
}

/// Extract `idnum2itemresnametable.txt` from the GRF in-memory and join it
/// with rAthena item DBs to produce a Korean res name → AegisName map.
fn build_rathena_lookup(
    grf: &mut Grf<fs::File>,
    rathena_db: Option<&Path>,
) -> Result<HashMap<String, String>> {
    // Find the name table entry.
    let table_entry = grf
        .entries
        .iter()
        .find(|e| e.internal_path.eq_ignore_ascii_case(ITEM_RES_TABLE_PATH))
        .map(|e| grf::GrfEntry {
            internal_path: e.internal_path.clone(),
            pack_size: e.pack_size,
            length_aligned: e.length_aligned,
            real_size: e.real_size,
            entry_type: e.entry_type,
            data_offset: e.data_offset,
        });

    let Some(table_entry) = table_entry else {
        if rathena_db.is_some() {
            eprintln!("WARN: {ITEM_RES_TABLE_PATH} not found in GRF; skipping rAthena lookup");
        }
        return Ok(HashMap::new());
    };

    let table_data = grf
        .read_entry(&table_entry)
        .context("reading idnum2itemresnametable.txt from GRF")?;

    let res_table = rathena::parse_item_res_table(&table_data);
    println!("GRF item res table: {} entries", res_table.len());

    let Some(db_path) = rathena_db else {
        return Ok(HashMap::new());
    };

    // Load rAthena item DBs (re/ preferred).
    let db_files = [
        "re/item_db_equip.yml",
        "re/item_db_usable.yml",
        "re/item_db_etc.yml",
        "pre-re/item_db_equip.yml",
    ];

    let rathena_dbs: Vec<HashMap<u32, String>> = db_files
        .iter()
        .map(|f| rathena::parse_rathena_item_db(&db_path.join(f)))
        .filter(|m| !m.is_empty())
        .collect();

    Ok(rathena::build_res_to_aegis(&res_table, &rathena_dbs))
}
