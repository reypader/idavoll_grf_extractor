# idavoll-idavoll-grf-extractoror

Extracts a Ragnarok Online `.grf` archive to disk with Korean→English filename
translation. Designed as the upstream step before running
[idavoll-sprite-exporter](../idavoll_sprite_exporter/README.md).

## Build

```sh
cargo build --release
# Binary: target/release/idavoll-grf-extractor
```

## Usage

```
idavoll-grf-extractor [OPTIONS] <grf>

Arguments:
  <grf>  Path to the .grf file

Options:
  -o, --output <DIR>            Output directory [default: extracted]
  -t, --translations <TOML>    Path to translations.toml [default: translations.toml]
      --rathena-db <PATH>       Path to rAthena db/ directory
      --headgear-slots <PATH>   Where to write the headgear slots file (requires --rathena-db)
                                [default: headgear_slots.toml]
      --miss-log <PATH>         Where to write untranslated segments [default: miss_log.toml]
      --dry-run                 Translate paths without writing files (still writes miss log)
  -v, --verbose                 Print each file as it is extracted
  -h, --help                    Print help
```

### Basic extraction

```sh
idavoll-grf-extractor data.grf -o extracted/
```

Extracts all files to `extracted/`, translating Korean path segments to English
using `translations.toml`. Any segments that could not be translated are written
to `miss_log.toml` for later enrichment.

### With rAthena item name resolution and headgear slots

```sh
idavoll-grf-extractor data.grf -o extracted/ --rathena-db /path/to/rathena/db
```

When `--rathena-db` is provided, two additional things happen automatically:

1. **Item name translation** — reads `idnum2itemresnametable.txt` from the GRF
   and cross-references it with rAthena item databases to translate Korean item
   resource names (garment directories, item sprites, etc.) to AegisNames.

2. **`headgear_slots.toml` generation** — reads `accname_eng.lub` from the GRF
   and joins it with rAthena headgear item data to produce the slot lookup file
   consumed by `idavoll-sprite-exporter scan`. Written to `headgear_slots.toml` by
   default; override with `--headgear-slots`.

The `--rathena-db` path should point to the rAthena `db/` directory, which
contains subdirectories `re/` and `pre-re/`. The following files are read:

| File | Purpose |
|---|---|
| `re/item_db_equip.yml` | Equipment items (armor, weapons, garments, headgear) |
| `re/item_db_usable.yml` | Usable/consumable items |
| `re/item_db_etc.yml` | Miscellaneous items |
| `pre-re/item_db_equip.yml` | Pre-renewal equipment (fallback) |

rAthena source: https://github.com/rathena/rathena

### Dry run

Useful for previewing the translated output and generating the miss log before
committing to a full extraction:

```sh
idavoll-grf-extractor data.grf --dry-run --rathena-db /path/to/rathena/db
```

## Translation pipeline

Each GRF internal path (e.g. `data\sprite\인간족\몸통\남\novice_남.spr`) is
translated segment by segment:

1. **Pure ASCII** — kept as-is (most job names, weapon names, and English content
   are already ASCII in the GRF).
2. **`translations.toml` lookup** — whole segment matched against the hand-curated
   dictionary. Takes priority over all other sources.
3. **rAthena lookup** — whole segment matched against the Korean→AegisName map
   built from `idnum2itemresnametable.txt` + rAthena item DBs.
4. **Token-level fallback** — the segment is split on `_` and steps 1–3 are
   applied to each token individually (handles compound names like `novice_남`
   → `novice_male`).
5. **Miss** — any token that could not be translated is kept in its original
   Korean form and logged to the miss log.

Result: `data/sprite/human/body/male/novice_male.spr`

## Additional resources

### `translations.toml`

The hand-curated dictionary lives at `translations.toml` next to the binary. It
covers:

- Top-level GRF categories (`인간족` → `human`, `몬스터` → `monster`, etc.)
- Human sprite sub-directories (`몸통` → `body`, `머리통` → `head`)
- Gender tokens (`남` → `male`, `여` → `female`)
- All base and third-job class names
- Job mount variants (`룬드래곤` → `rune_dragon`, `레인져늑대` → `ranger_wolf`, etc.)

To add new entries, append to `[known]` in `translations.toml` and re-run.

### `miss_log.toml`

After each run a `miss_log.toml` is written listing every Korean segment that
could not be translated:

```toml
# Translation misses — fill in the English values and move entries to translations.toml
[known]
"검의날개" = ""
"요정의파란날개" = ""
```

Fill in the empty values and move the entries into `translations.toml` to
resolve them on the next run. Over time this enriches the dictionary for content
not covered by rAthena (newer cash-shop items, costume garments, etc.).

## Output structure

The extracted output mirrors the GRF's internal directory tree with Korean
segments replaced by English equivalents. The `sprite/` subtree — consumed by
`idavoll-sprite-exporter` — looks like:

```
extracted/
└── data/
    └── sprite/
        ├── human/
        │   ├── body/
        │   │   ├── male/          # Body sprites per job (novice_male.spr, ...)
        │   │   └── female/
        │   ├── head/
        │   │   ├── male/          # Numbered head sprites (1_male.spr, ...)
        │   │   └── female/
        │   ├── swordsman/         # Weapon sprites per job
        │   ├── mage/
        │   └── ...
        ├── accessory/
        │   ├── male/              # Headgear sprites (m_ribbon.spr, ...)
        │   └── female/
        ├── robe/
        │   └── <garment_name>/    # Garment sprites per name/job/gender
        │       ├── male/
        │       └── female/
        ├── monster/               # Monster sprites
        ├── item/                  # Item icon sprites
        └── ...
```

Point `idavoll-sprite-exporter scan` at `extracted/data/` as the `data_root`.
