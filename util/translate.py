#!/usr/bin/env python3
"""
Batch translate Korean RO GRF tokens to English using the Claude API.

Usage:
    python translate.py [--model sonnet|haiku] [--batch-size 500] [--dry-run] [-v]

Environment:
    ANTHROPIC_API_KEY  -- required
"""

import argparse
import datetime
import json
import os
import re
import sys
import time
import random
import tomllib
import anthropic

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MODEL_MAP = {
    "sonnet": "claude-sonnet-4-6",
    "haiku":  "claude-haiku-4-5-20251001",
}

DEFAULT_BATCH_SIZE = 200

SYSTEM_PROMPT = """\
You are a translation assistant for Ragnarok Online (RO), a Korean MMORPG.
You are translating Korean GRF (Gravity Resource File) path tokens into English.

These tokens are used as file/directory name segments in the game's asset archive.
The output will be used directly in file paths, so values must be:
- snake_case (lowercase, words separated by underscores)
- No spaces, no hyphens, no special characters
- Concise but meaningful English game terminology

Key translation rules:
1. "카드" suffix means "_card" (e.g., "히드라카드" -> "hydra_card")
2. Common item parts: "모자" = hat, "머리" = head/hair, "안경" = glasses, "귀" = ear
3. Numbers at end stay as suffix: "가로등01" -> "street_lamp_01"
4. Prefixes to preserve:
   - "c" prefix = costume variant ("c고양이귀모자" -> "c_cat_ear_hat")
   - "mo-" prefix = map object ("mo-피라미드1-바닥" -> "mo_pyramid_1_floor")
   - "rwc" prefix = RWC event item ("rwc" stays as-is)
   - "att" prefix = arena/battle item (preserve)
   - "g" prefix = game mechanic token (preserve)
5. Gender markers: "(남)" = "_male", "(여)" = "_female"
6. Hyphens and spaces in original key become underscores in output
7. "~$" prefix is a temp artifact; use just the Korean part for translation
8. Environment/map objects: 벽 = wall, 바닥 = floor, 기둥 = pillar, 나무 = tree,
   가로등 = street_lamp, 의자 = chair, 상자 = box, 난간 = railing, 계단 = stairs
9. Item types: 포션 = potion, 스크롤 = scroll, 반지 = ring, 목걸이 = necklace,
   부츠 = boots, 망토 = cloak, 가방 = bag, 날개 = wings, 리본 = ribbon
10. Korean phonetic spellings of English game terms: transliterate back to English
    (e.g., "엘프" -> "elf", "드래곤" -> "dragon", "엔젤" -> "angel")

Every key in the input MUST have a corresponding key in the output JSON, even if
the value is an approximation. Do not omit keys.

Respond ONLY with a JSON object mapping each Korean key exactly as given to its
English snake_case translation. No explanation, no markdown fences, just the JSON.
"""

APPEND_HEADER = """
# ---------------------------------------------------------------------------
# Auto-translated via Claude API ({date})
# Model: {model}
# Keys translated: {count}
# ---------------------------------------------------------------------------
"""


# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

def parse_args():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--model", choices=["sonnet", "haiku"], default="sonnet",
                   help="Model to use (default: sonnet)")
    p.add_argument("--batch-size", type=int, default=DEFAULT_BATCH_SIZE,
                   help="Keys per API call (default: 500)")
    p.add_argument("--miss-log", default="../miss_log.toml",
                   help="Path to miss_log.toml")
    p.add_argument("--translations", default="../translations.toml",
                   help="Path to translations.toml")
    p.add_argument("--progress", default="./translation_progress.json",
                   help="Path to progress file for resume support")
    p.add_argument("--dry-run", action="store_true",
                   help="Show batch plan without making API calls")
    p.add_argument("--no-append", action="store_true",
                   help="Translate and save progress only; skip writing translations.toml")
    p.add_argument("-v", "--verbose", action="store_true",
                   help="Print per-batch token usage")
    p.add_argument("--debug", action="store_true",
                   help="Print raw API response for the first batch (useful for diagnosing parse failures)")
    return p.parse_args()


# ---------------------------------------------------------------------------
# TOML reading
# ---------------------------------------------------------------------------

def load_miss_keys(path: str) -> list[str]:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    return [k for k, v in data.get("known", {}).items() if v == ""]


def load_existing_keys(path: str) -> set[str]:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    return {k for k, v in data.get("known", {}).items() if v != ""}


# ---------------------------------------------------------------------------
# Progress file
# ---------------------------------------------------------------------------

def load_progress(path: str) -> dict:
    if os.path.exists(path):
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    return {
        "completed_batches": [],
        "translations": {},
        "failed_keys": [],
    }


def save_progress(path: str, progress: dict):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(progress, f, ensure_ascii=False, indent=2)


# ---------------------------------------------------------------------------
# API call with retry
# ---------------------------------------------------------------------------

def call_claude(client: anthropic.Anthropic, model_id: str, batch_keys: list[str], verbose: bool, debug: bool = False) -> tuple[dict, object]:
    user_content = (
        "Translate the following Ragnarok Online GRF token keys from Korean to English "
        "snake_case.\nReturn a JSON object where each key is the original Korean token "
        "and each value is its English snake_case translation.\n\n"
        "Keys to translate:\n"
        + json.dumps(batch_keys, ensure_ascii=False, indent=2)
    )

    response = None
    for attempt in range(5):
        try:
            response = client.messages.create(
                model=model_id,
                max_tokens=16384,
                system=SYSTEM_PROMPT,
                messages=[{"role": "user", "content": user_content}],
            )
            break
        except anthropic.RateLimitError as e:
            retry_after = None
            if hasattr(e, "response") and e.response is not None:
                retry_after = e.response.headers.get("retry-after")
            wait = int(retry_after) if retry_after else 60
            print(f"\n  [rate limit] waiting {wait}s...", file=sys.stderr)
            time.sleep(wait)
        except (anthropic.APIConnectionError, anthropic.InternalServerError) as e:
            if attempt == 4:
                raise
            wait = min((2 ** attempt) * 2 + random.uniform(0, 1), 120)
            print(f"\n  [{type(e).__name__}] retry {attempt + 1}/5 in {wait:.1f}s", file=sys.stderr)
            time.sleep(wait)

    if response is None:
        raise RuntimeError("Max retries exceeded with no response")

    text = response.content[0].text.strip()

    if debug:
        print(f"\n  [debug] stop_reason={response.stop_reason}")
        print(f"  [debug] raw response (first 500 chars):\n{text[:500]}")

    text = re.sub(r"^```(?:json)?\s*", "", text, flags=re.DOTALL)
    text = re.sub(r"\s*```$", "", text, flags=re.DOTALL)

    try:
        result = json.loads(text)
    except json.JSONDecodeError:
        m = re.search(r"\{.*\}", text, re.DOTALL)
        if m:
            try:
                result = json.loads(m.group())
            except json.JSONDecodeError:
                result = {}
        else:
            result = {}

    if verbose or debug:
        u = response.usage
        print(f"\n  tokens: input={u.input_tokens} output={u.output_tokens}  stop={response.stop_reason}")

    return result, response.usage


# ---------------------------------------------------------------------------
# Value normalisation
# ---------------------------------------------------------------------------

def normalise(value: str) -> str:
    v = value.strip().lower()
    v = re.sub(r"[\s\-]+", "_", v)
    v = re.sub(r"[^a-z0-9_]", "", v)
    v = re.sub(r"_+", "_", v).strip("_")
    return v


# ---------------------------------------------------------------------------
# TOML append
# ---------------------------------------------------------------------------

def append_to_translations(path: str, new_entries: dict, model_id: str, failed_keys: list[str]):
    header = APPEND_HEADER.format(
        date=datetime.date.today().isoformat(),
        model=model_id,
        count=len(new_entries),
    )
    lines = [header]
    for key in sorted(new_entries.keys()):
        value = new_entries[key].replace("\\", "\\\\").replace('"', '\\"')
        lines.append(f'"{key}" = "{value}"\n')

    if failed_keys:
        lines.append("\n# Keys that could not be confidently translated (review manually):\n")
        for key in sorted(failed_keys):
            lines.append(f'# "{key}" = ""\n')

    with open(path, "a", encoding="utf-8") as f:
        f.write("".join(lines))

    print(f"Appended {len(new_entries)} entries to {path}")
    if failed_keys:
        print(f"Logged {len(failed_keys)} failed keys as comments for manual review.")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    args = parse_args()
    model_id = MODEL_MAP[args.model]

    miss_log_path = os.path.abspath(args.miss_log)
    translations_path = os.path.abspath(args.translations)
    progress_path = os.path.abspath(args.progress)

    all_miss_keys = load_miss_keys(miss_log_path)
    existing_keys = load_existing_keys(translations_path)
    keys_to_translate = [k for k in all_miss_keys if k not in existing_keys]

    print(f"miss_log keys:      {len(all_miss_keys)}")
    print(f"already translated: {len(existing_keys)}")
    print(f"to translate:       {len(keys_to_translate)}")

    progress = load_progress(progress_path)
    accumulated: dict = dict(progress.get("translations", {}))
    failed_keys: list = list(progress.get("failed_keys", []))
    completed: set = set(progress.get("completed_batches", []))

    # Remove already-accumulated keys from the to-translate list
    remaining = [k for k in keys_to_translate if k not in accumulated and k not in failed_keys]

    batches = [
        remaining[i : i + args.batch_size]
        for i in range(0, len(remaining), args.batch_size)
    ]
    total_batches = len(batches)

    if args.dry_run:
        print(f"\nDry run: {total_batches} batches of up to {args.batch_size} keys")
        print(f"Model:   {model_id}")
        for i, b in enumerate(batches):
            print(f"  Batch {i:2d}: {len(b):4d} keys  ({b[0]!r} ... {b[-1]!r})")
        return

    if not keys_to_translate:
        print("Nothing to translate.")
    elif not remaining:
        print("All keys already translated or failed. Proceeding to append only.")
    else:
        api_key = os.environ.get("ANTHROPIC_API_KEY")
        if not api_key:
            print("ERROR: ANTHROPIC_API_KEY environment variable not set.", file=sys.stderr)
            sys.exit(1)

        client = anthropic.Anthropic(api_key=api_key, max_retries=4)

        for i, batch_keys in enumerate(batches):
            print(f"[{i + 1}/{total_batches}] Translating {len(batch_keys)} keys...", end=" ", flush=True)

            try:
                result, usage = call_claude(client, model_id, batch_keys, args.verbose, debug=args.debug and i == 0)
            except Exception as e:
                print(f"\n  FAILED batch {i}: {e}", file=sys.stderr)
                for k in batch_keys:
                    if k not in failed_keys:
                        failed_keys.append(k)
                completed.add(i)
                progress["completed_batches"] = sorted(completed)
                progress["failed_keys"] = failed_keys
                progress["translations"] = accumulated
                save_progress(progress_path, progress)
                continue

            batch_translated = {}
            batch_failed = []
            for key in batch_keys:
                raw = result.get(key, "")
                if not raw or not raw.strip():
                    batch_failed.append(key)
                    continue
                norm = normalise(raw)
                if not norm:
                    batch_failed.append(key)
                    continue
                if norm != raw.strip().lower():
                    print(f"\n  [normalised] {key!r}: {raw!r} -> {norm!r}", end="")
                batch_translated[key] = norm

            accumulated.update(batch_translated)
            failed_keys.extend(batch_failed)
            completed.add(i)

            print(f"ok ({len(batch_translated)} ok, {len(batch_failed)} failed)")

            progress["completed_batches"] = sorted(completed)
            progress["total_batches"] = total_batches
            progress["batch_size"] = args.batch_size
            progress["model"] = model_id
            progress["translations"] = accumulated
            progress["failed_keys"] = failed_keys
            save_progress(progress_path, progress)

        print(f"\nDone. {len(accumulated)} translated, {len(failed_keys)} failed.")

    if not args.no_append:
        if accumulated:
            append_to_translations(translations_path, accumulated, model_id, failed_keys)
        else:
            print("No translations to append.")
    else:
        print(f"--no-append set: results saved to {progress_path}")


if __name__ == "__main__":
    main()
