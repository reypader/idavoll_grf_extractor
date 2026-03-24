# GRF Extractor — Research Notes

## Project Goal

Extract Ragnarok Online `.grf` archive files with Korean→English filename translation,
producing an on-disk directory tree that the `idavoll_sprite_exporter` tool can consume.

---

## GRF Binary Format

### Header (46 bytes)
```
"Master of Magic\0"  15 bytes  magic signature
key                  15 bytes  unused (keyless DES variant — key is hardcoded in decrypt.rs)
table_offset         u32 LE    byte offset of the file table from end of header
skip_count           u32 LE    unused
raw_count            u32 LE    total entry count
version              u32 LE    must be 0x0200
```

### File Table
Stored at `HEADER_SIZE + table_offset`. Contains a zlib-compressed block (pack_size +
real_size + data), which decompresses into a flat sequence of entry records.

### Entry Record
```
filename             null-terminated CP949 string (backslash path separator)
pack_size            u32 LE    compressed size in archive
length_aligned       u32 LE    decompressed size rounded up to 8-byte alignment
real_size            u32 LE    actual decompressed size
entry_type           u8        encryption flags (see below)
data_offset          u32 LE    byte offset of compressed data from start of file
```

### Entry Type Flags
| Flag | Value | Meaning |
|------|-------|---------|
| `TYPE_FILE` | 0x01 | Regular file (entries without this are directory markers — skip) |
| `TYPE_ENCRYPT_MIXED` | 0x02 | Fully encrypted (full DES on first 20 blocks + cycle pattern) |
| `TYPE_ENCRYPT_HEADER` | 0x04 | Header-only encrypted (first 20 blocks only) |

### Decryption
Keyless DES variant (see `decrypt.rs`, based on ROBrowser's GameFileDecrypt.js).
- `ENCRYPT_MIXED`: first 20 blocks full DES, remaining blocks cycle-based (period = digit count of pack_size)
- `ENCRYPT_HEADER`: first 20 blocks only

After decryption, data is decompressed with zlib. Result size = `real_size`.

---

## Translation System

### Strategy (per path segment, applied left-to-right)
1. **Pure ASCII** → keep as-is (no lookup needed)
2. **Whole segment** → check `translations.toml [known]` map
3. **Whole segment** → check rAthena-derived map (Korean res name → AegisName)
4. **Strip extension, retry** whole base without extension
5. **Token-by-token** → split on `_`, apply steps 1–3 per token; log untranslated tokens to `miss_log.toml`

Backslashes are converted to forward slashes in the output path.

### translations.toml
Hand-curated dictionary at `idavoll_grf_extractor/translations.toml`. Structure:
```toml
[known]
"한국어" = "english"
```
- Whole-segment entries take priority over token-by-token
- Compound Korean words with no `_` separator must be entered as whole-segment entries
- Add new entries when `miss_log.toml` shows untranslated segments in relevant paths

### miss_log.toml
Written after each run. Contains all Korean tokens encountered in GRF paths that had no
translation entry. Format is a stub `[known]` block with blank values — fill in and merge
into `translations.toml`. Only tokens in converter-relevant paths (`sprite/`, `imf/`) need
to be translated; other paths (maps, sounds, UI) can stay Korean.

### rAthena fallback
When `--rathena-db` is provided, idavoll_grf_extractor extracts `idnum2itemresnametable.txt` from the
GRF and joins it with rAthena item DB YAMLs to build a Korean res name → AegisName map.
This covers item sprite filenames without needing manual `translations.toml` entries.

---

## Bundle System

Defined in `bundles.toml`. Allows selective extraction via `--extract <name>[,<name>]`.

### Bundle matching rules (per entry, union of all rules)
- `path_prefixes`: entry's translated path starts with any of these
- `extensions`: entry's translated path ends with `.{ext}` (case-insensitive)

### Predefined bundles (`bundles.toml`)
| Bundle | Includes |
|--------|----------|
| `sprite` | `data/sprite/` and `data/imf/` |
| `map` | `data/texture/` + `.gat`, `.gnd`, `.rsw` files |

Omitting `--extract` extracts everything (default behaviour, preserves backward compatibility).

### Adding new bundles
Edit `bundles.toml` — no code changes needed. Bundle definitions are loaded at runtime.

---

## rAthena Integration

### Headgear slots (`headgear_slots.toml`)
Generated when `--rathena-db` is provided. Combines:
- `accname_eng.lub` extracted from the GRF (Lua bytecode scanned for null-terminated ASCII
  strings matching `_[A-Z][A-Z0-9_]+` pattern — these are the accname identifiers)
- `re/item_db_equip.yml` from rAthena (provides view ID and slot per headgear item)

Output maps each headgear `accname` to its view ID and slot (`Head_Top` / `Head_Mid` / `Head_Low`).
Used by `idavoll_sprite_exporter scan` to assign headgear slot indices.

### Item res name table
`data\idnum2itemresnametable.txt` maps numeric item IDs to Korean res names. Joined with
rAthena DB to produce Korean → AegisName lookup for item sprite filename translation.

---

## Output Directory Structure

```
extracted/
└── data/
    ├── imf/              IMF anchor files (flat, stem matches body sprite stem)
    ├── sprite/
    │   ├── shadow.spr / shadow.act
    │   ├── human/
    │   │   ├── body/{male,female}/      body sprites + costume_{n}/ subdirs
    │   │   ├── head/{male,female}/      head sprites
    │   │   ├── mercenary/               gender-neutral mercenary weapon sprites
    │   │   └── {job}/                   weapon overlay sprites per job
    │   ├── accessory/{male,female}/     headgear sprites
    │   ├── robe/{name}/{male,female}/   garment sprites per job
    │   ├── shield/{job}/                shield sprites
    │   ├── monster/                     flat; ~1265 sprite pairs
    │   ├── npc/                         flat; ~1130 sprite pairs
    │   ├── homun/                       flat; 21 pairs (homunculus)
    │   ├── item/                        flat; ~6202 pairs (drop sprites, heavily Korean)
    │   └── doram/
    │       ├── body/{male,female}/      Summoner body sprites
    │       └── head/{male,female}/      Summoner head sprites
    ├── texture/          map textures
    ├── model/            3D models
    └── ...               maps (.gat/.gnd/.rsw), sounds, UI, Lua files, etc.
```

---

## Known GRF Artifacts

### Duplicate / mis-copied files
The GRF occasionally stores the same file twice under different paths. Before adding scan
handling for an unexpected file, verify with MD5 that it is a duplicate of a known file.

| File | Canonical location | Notes |
|------|--------------------|-------|
| `data/sprite/human/body/female/assassin_cross_female.imf` | `data/imf/assassin_cross_female.imf` | Only co-located IMF; identical MD5 |
| `data/sprite/human/body/male/lord_knight_남''.spr` | `lord_knight_male.spr` (same dir) | CP949 mojibake artifact; `''` in stem |
| `data/sprite/human/pecopeco_paladin/pecopeco_crusader_{f,m}.spr` | `human/pecopeco_crusader/` | GRF stores both; scan picks up from correct dir |
| `data/sprite/human/body/{gender}/rebellion_{gender}_{weapon}.spr` | `human/rebellion/` | Mis-copied weapon files; no Rebellion body overlay system |

### `_female` / `_male` directory anomaly
idavoll_grf_extractor may produce `human/body/_female/` alongside `female/` when the GRF uses `_여`
instead of `여` for the gender segment. Files are byte-identical duplicates. The converter
scans only `female/` and `male/` — `_female/` is silently ignored.

### Korean-named files in imf/
`data/imf/` contains `구페코_crusader_{m,f}.imf` and `신페코_crusader_{m,f}.imf` from a
previous extraction where `구페코` / `신페코` were not yet in `translations.toml`. These are
now translated correctly as `pecopeco_crusader` / `pecopeco_paladin` after re-extraction.

---

## CLI Reference

```
idavoll-grf-extractor <GRF> [OPTIONS]

Arguments:
  <GRF>                        Path to the .grf file

Options:
  -o, --output <DIR>           Output directory [default: extracted]
  -t, --translations <PATH>    translations.toml [default: translations.toml]
      --rathena-db <PATH>      rAthena db/ directory (enables headgear slots + item lookup)
      --headgear-slots <PATH>  Output headgear_slots.toml [default: headgear_slots.toml]
      --miss-log <PATH>        Output miss log [default: miss_log.toml]
      --bundles <PATH>         Bundle definitions [default: bundles.toml]
      --extract <NAMES>        Comma-separated bundle names to extract (omit = extract all)
      --dry-run                Translate paths without writing files (still writes miss log)
  -v, --verbose                Print each extracted file path
```

### Typical workflow
```sh
# Full extraction with rAthena DB (generates headgear_slots.toml)
idavoll-grf-extractor data.grf \
  -o extracted/ \
  --translations translations.toml \
  --rathena-db /path/to/rathena/db \
  --headgear-slots headgear_slots.toml

# Sprite-only re-extraction (faster iteration)
idavoll-grf-extractor data.grf \
  -o extracted/ \
  --translations translations.toml \
  --extract sprite
```
