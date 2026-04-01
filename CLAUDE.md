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
| `sprite` | `data/sprite/`, `data/imf/`, `data/palette/` |
| `map` | `data/texture/`, `data/model/` + `.gat`, `.gnd`, `.rsw` files |
| `sound` | `data/wav/` |

Omitting `--extract` extracts everything (default behaviour, preserves backward compatibility).

### Translation control per bundle
Each bundle has an optional `translate` boolean (default `true`). When set to `false`, files
matching that bundle's `path_prefixes` are written with their original Korean filenames, with
only backslash-to-slash path separator conversion applied. `extensions`-matched entries are
unaffected by this flag (extension matching happens after translation).

Current defaults: `sprite` translates; `map` and `sound` do not.

`bundles.toml` is always loaded at startup, even when `--extract` is omitted, so translation
passthrough applies to full extractions as well.

### Adding new bundles
Edit `bundles.toml` — no code changes needed. Bundle definitions are loaded at runtime.

### Known bundle gaps (not yet covered)

These paths exist in the GRF but are not matched by any current bundle. Recorded for future
bundling decisions.

**Sprites outside `data/sprite/`**
`data/luafiles514/lua files/sprite/` contains `.spr` and `.act` files (warlock,
fox_warlock body sprites; some robe variants). Bitwise comparison against `data/sprite/`
shows these are older/partial versions superseded by the canonical files:
- `warlock_{female,male}.act/.spr` (4 files): byte-identical to `data/sprite/` — safe to ignore.
- `fox_warlock_{female,male}.act` (2 files): larger in `luafiles514` (48276 vs 44628 bytes);
  `.spr` files differ in content but are same size. Canonical `data/sprite/` version should be used.
- Robe sprites (8 files): `luafiles514` `.spr` files are roughly half the size of
  `data/sprite/` equivalents (e.g. 7219 vs 13152 bytes) — clearly older/incomplete.
  The `dancer_pants_female.spr` differs by 2 bytes (minor patch).

Verdict: `data/sprite/` is canonical for all of these. The sprite bundle does not need to
cover `data/luafiles514/lua files/sprite/`.

**Root-level `.txt` data tables** (40 files at `data/*.txt`)

Map-relevant:
- `mapnametable.txt` — internal map name → display name
- `mp3nametable.txt` — map → BGM filename
- `fogparametertable.txt` — per-map fog settings
- `indoorrswtable.txt` — indoor map flags
- `mapobjlighttable.txt` — map object lighting
- `mappostable.txt` — NPC/warp positions
- `viewpointtable.txt` — camera viewpoints

Item/game data:
- `idnum2itemresnametable.txt`, `idnum2itemdisplaynametable.txt`, `idnum2itemdesctable.txt`
- `num2itemresnametable.txt`, `num2itemdisplaynametable.txt`, `num2itemdesctable.txt` (older equivalents)
- `carditemnametable.txt`, `cardprefixnametable.txt`, `cardpostfixnametable.txt`
- `itemslottable.txt`, `itemslotcounttable.txt`, `itemparamtable.txt`
- `buyingstoreitemlist.txt`, `metalprocessitemlist.txt`, `metalprocessitemtable.txt`
- `ItemMoveInfoV5.txt`, `resnametable.txt`, `bookitemnametable.txt`, `num2cardillustnametable.txt`

Skill data:
- `skillnametable.txt`, `skilldesctable.txt`, `skilldesctable2.txt`
- `skilltreeview.txt`, `jobinheritlist.txt`, `leveluseskillspamount.txt`

Misc: `etcinfo.txt`, `manner.txt`, `tipoftheday.txt`, `questid2display.txt`,
`ba_frostjoke.txt`, `dc_scream.txt`, `exceptionminimapnametable.txt`

**Lua / script files**
- `data/luafiles514/` (272 files: `.lub`, `.lua`, plus the sprites above and 5 `.wav` in `effecttool/wav/`)
- `data/lua files/` (61 files: older/duplicate Lua directory)
- `data/ai/` (3 Lua monster AI scripts)

**Other**
- `data/user_interface/` — 15 `.bmp` UI illustration images
- `data/book/` — 45 `.txt` in-game book text files
- `data/video/` — 1 `.bik` intro video

---

## rAthena Integration

### Headgear slots (`headgear_slots.toml`)
Generated when `--rathena-db` is provided. Source: `re/item_db_equip.yml` only — no GRF
Lua scanning required.

- Items with `View > 0` and at least one head location (`Head_Top`, `Head_Mid`, `Head_Low`)
  are grouped by view ID.
- The `accname` for each view group is the AegisName (lowercased) of the lowest-ID item in
  that group (the original/canonical item whose AegisName matches the sprite identifier).
- Canonical slot: if all items in the group share the same location, that location is used;
  conflicting locations default to `Head_Top`.

Output maps each headgear view ID to its `accname`, slot, and all item IDs.
Used by `idavoll_sprite_exporter scan` to assign headgear slot indices.

### Weapon types (`weapon_types.toml`)
Generated when `--rathena-db` is provided. Source: `re/item_db_equip.yml`.

- Items with `Type: Weapon` are grouped by `SubType`.
- Each SubType maps to a numeric weapon type ID (from rAthena `src/map/pc.hpp`) and a
  sprite directory name matching the translated GRF path segment:

  | SubType | ID | sprite name |
  |---------|----|-------------|
  | `Dagger` | 1 | `dagger` |
  | `1hSword` | 2 | `sword` |
  | `2hSword` | 3 | `two_handed_sword` |
  | `1hSpear` | 4 | `spear` |
  | `2hSpear` | 5 | `two_handed_spear` |
  | `1hAxe` | 6 | `axe` |
  | `2hAxe` | 7 | `two_handed_axe` |
  | `Mace` | 8 | `mace` |
  | `Staff` | 10 | `staff` |
  | `Bow` | 11 | `bow` |
  | `Knuckle` | 12 | `knuckle` |
  | `Musical` | 13 | `musical` |
  | `Whip` | 14 | `whip` |
  | `Book` | 15 | `book` |
  | `Katar` | 16 | `katar` |
  | `Revolver` | 17 | `revolver` |
  | `Rifle` | 18 | `rifle` |
  | `Gatling` | 19 | `gatling_gun` |
  | `Shotgun` | 20 | `shotgun` |
  | `Grenade` | 21 | `grenade_launcher` |
  | `Huuma` | 22 | `fuuma_shuriken` |
  | `2hStaff` | 23 | `two_handed_staff` |

  ID 9 (`W_2HMACE`) has no items in the rAthena DB and is absent from the output.

Output lists weapon types sorted by ID, each with the sprite `name` and all item IDs.
Used by `idavoll_sprite_exporter` to categorize weapon overlay sprites.

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
      --rathena-db <PATH>      rAthena db/ directory (enables headgear slots + weapon types + item lookup)
      --headgear-slots <PATH>  Output headgear_slots.toml [default: headgear_slots.toml]
      --weapon-types <PATH>    Output weapon_types.toml [default: weapon_types.toml]
      --miss-log <PATH>        Output miss log [default: miss_log.toml]
      --bundles <PATH>         Bundle definitions [default: bundles.toml]
      --extract <NAMES>        Comma-separated bundle names to extract (omit = extract all)
      --dry-run                Translate paths without writing files (still writes miss log)
  -v, --verbose                Print each extracted file path
```

### Typical workflow
```sh
# Full extraction with rAthena DB (generates headgear_slots.toml + weapon_types.toml)
idavoll-grf-extractor data.grf \
  -o extracted/ \
  --translations translations.toml \
  --rathena-db /path/to/rathena/db \
  --headgear-slots headgear_slots.toml \
  --weapon-types weapon_types.toml

# Sprite-only re-extraction (faster iteration)
idavoll-grf-extractor data.grf \
  -o extracted/ \
  --translations translations.toml \
  --extract sprite
```
