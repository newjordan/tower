#!/usr/bin/env bash
set -euo pipefail

SOURCE_URL="${SOURCE_URL:-https://opengameart.org/sites/default/files/lpc_-_the_office.zip}"
SOURCE_ZIP_PATH="${SOURCE_ZIP_PATH:-assets/public/v1/sources/lpc_revised_the_office/lpc_the_office.zip}"
EXTRACT_DIR="${EXTRACT_DIR:-assets/public/v1/sources/lpc_revised_the_office/extracted}"
OUT_DIR="${OUT_DIR:-assets/public/v1/imported/lpc_revised_the_office}"
CELL_SIZE="${CELL_SIZE:-32}"
DOWNLOAD_SOURCE="${DOWNLOAD_SOURCE:-1}"

mkdir -p "$(dirname "$SOURCE_ZIP_PATH")"
mkdir -p "$EXTRACT_DIR"
mkdir -p "$OUT_DIR/tiles"

if [[ ! -f "$SOURCE_ZIP_PATH" ]]; then
  if [[ "$DOWNLOAD_SOURCE" == "1" ]]; then
    echo "downloading source: $SOURCE_URL"
    curl -L --fail -sS "$SOURCE_URL" -o "$SOURCE_ZIP_PATH"
  else
    echo "missing source zip: $SOURCE_ZIP_PATH"
    echo "set DOWNLOAD_SOURCE=1 or provide SOURCE_ZIP_PATH"
    exit 1
  fi
fi

python3 - "$SOURCE_ZIP_PATH" "$EXTRACT_DIR" "$OUT_DIR" "$CELL_SIZE" "$SOURCE_URL" <<'PY'
import json
import re
import shutil
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path

from PIL import Image, ImageDraw

source_zip = Path(sys.argv[1])
extract_dir = Path(sys.argv[2])
out_dir = Path(sys.argv[3])
cell_size = int(sys.argv[4])
source_url = sys.argv[5]

extract_dir.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(source_zip) as zf:
    zf.extractall(extract_dir)

tiles_dir = out_dir / "tiles"
tiles_dir.mkdir(parents=True, exist_ok=True)

def slugify(raw: str) -> str:
    normalized = re.sub(r"[^a-z0-9]+", "_", raw.lower())
    normalized = re.sub(r"_+", "_", normalized).strip("_")
    return normalized or "asset"

entries = []
png_files = sorted(extract_dir.glob("*.png"))
for src in png_files:
    sheet = Image.open(src).convert("RGBA")
    width, height = sheet.size
    if width % cell_size != 0 or height % cell_size != 0:
        continue
    cols = width // cell_size
    rows = height // cell_size
    stem = slugify(src.stem)
    for row in range(rows):
        for col in range(cols):
            x0 = col * cell_size
            y0 = row * cell_size
            tile = sheet.crop((x0, y0, x0 + cell_size, y0 + cell_size))
            if tile.getchannel("A").getbbox() is None:
                continue
            asset_id = f"prop_lpc_{stem}_r{row}c{col}"
            out_path = tiles_dir / f"{asset_id}.png"
            tile.save(out_path)
            entries.append(
                {
                    "asset_id": asset_id,
                    "source_sheet": src.name,
                    "source_cell": {"row": row, "col": col},
                    "source_rect": {"x": x0, "y": y0, "w": cell_size, "h": cell_size},
                    "file": str(out_path).replace("\\", "/"),
                    "tags": [
                        "department_prop",
                        "environment_tile",
                        "lpc",
                        "office",
                        f"sheet_{stem}",
                    ],
                }
            )

by_id = {entry["asset_id"]: entry for entry in entries}
alias_map = {
    "prop_push_desk": "prop_lpc_desk_ornate_r0c0",
    "prop_pull_desk": "prop_lpc_desk_ornate_r0c1",
    "prop_commit_desk": "prop_lpc_desk_ornate_r0c2",
    "prop_review_desk": "prop_lpc_desk_ornate_r0c3",
    "prop_test_desk": "prop_lpc_desk_ornate_r0c4",
    "prop_planning_board": "prop_lpc_office_portraits_r0c0",
    "portrait_coding_neutral": "prop_lpc_office_portraits_r0c0",
    "portrait_coding_thinking": "prop_lpc_office_portraits_r0c1",
    "portrait_coding_warning": "prop_lpc_office_portraits_r1c0",
    "portrait_coding_success": "prop_lpc_office_portraits_r1c1",
    "ui_prestige_star": "prop_lpc_rotary_phones_r0c0",
    "ui_state_healthy": "prop_lpc_copy_machine_copy_light_r0c0",
    "ui_state_degraded": "prop_lpc_copy_machine_copy_light_r0c1",
    "ui_state_stalled": "prop_lpc_copy_machine_copy_light_r0c2",
    "ui_state_broken": "prop_lpc_copy_machine_copy_light_r0c3",
    "fx_spark_flicker": "prop_lpc_copy_machine_copy_light_r0c4",
    "fx_upload_burst": "prop_lpc_copy_machine_copy_light_r0c5",
    "fx_fail_pop": "prop_lpc_copy_machine_copy_light_r0c6",
    "openai_subscription_probe": "prop_lpc_copy_machine_copy_light_r0c7",
    "tile_floor_clean": "prop_lpc_liberated_palette_ramps_r1c1",
    "tile_window_night": "prop_lpc_tv_widescreen_r0c0",
}

alias_count = 0
for alias_id, base_id in alias_map.items():
    base = by_id.get(base_id)
    if base is None:
        continue
    alias_path = tiles_dir / f"{alias_id}.png"
    shutil.copyfile(base["file"], alias_path)
    entries.append(
        {
            "asset_id": alias_id,
            "alias_of": base_id,
            "source_sheet": base.get("source_sheet"),
            "source_cell": base.get("source_cell"),
            "source_rect": base.get("source_rect"),
            "file": str(alias_path).replace("\\", "/"),
            "tags": ["compat_alias", "lpc", "office"],
        }
    )
    alias_count += 1

manifest = {
    "pack_id": "lpc_revised_the_office",
    "pack_name": "[LPC Revised] The Office",
    "source_url": source_url,
    "source_file": str(source_zip).replace("\\", "/"),
    "license": "CC-BY-SA 3.0, OGA-BY 3.0",
    "cell_size": cell_size,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "assets": entries,
    "compatibility_aliases": alias_map,
}
with (out_dir / "manifest.json").open("w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")
with (out_dir / "tile_map.json").open("w", encoding="utf-8") as fh:
    json.dump({"assets": entries}, fh, indent=2)
    fh.write("\n")

# Write attribution text required for this pack.
attribution = "\n".join(
    [
        "Pack: [LPC Revised] The Office",
        "Source: https://opengameart.org/content/lpc-revised-the-office",
        "Author: Eliza Wyatt",
        "Additional note from source: Elements of the Ornate Desk and the Office Portraits use assets by Lanea Zimmerman (Sharm).",
        "License: CC-BY-SA 3.0 and OGA-BY 3.0",
    ]
)
(out_dir / "ATTRIBUTION.txt").write_text(attribution + "\n", encoding="utf-8")

# Quick alias review preview.
alias_assets = [a for a in entries if "compat_alias" in a.get("tags", [])]
if alias_assets:
    cols = 2
    card_w = 360
    card_h = 82
    pad = 12
    rows_needed = (len(alias_assets) + cols - 1) // cols
    board = Image.new(
        "RGBA",
        (cols * card_w + (cols + 1) * pad, rows_needed * card_h + (rows_needed + 1) * pad),
        (18, 21, 27, 255),
    )
    draw = ImageDraw.Draw(board)
    for idx, entry in enumerate(alias_assets):
        col = idx % cols
        row = idx // cols
        x = pad + col * (card_w + pad)
        y = pad + row * (card_h + pad)
        draw.rectangle((x, y, x + card_w, y + card_h), fill=(32, 36, 45, 255), outline=(104, 124, 184, 255), width=2)
        tile = Image.open(entry["file"]).convert("RGBA").resize((64, 64), Image.NEAREST)
        board.alpha_composite(tile, (x + 8, y + 8))
        draw.text((x + 82, y + 8), entry["asset_id"], fill=(232, 238, 247, 255))
        draw.text((x + 82, y + 30), f"base: {entry.get('alias_of', '?')}", fill=(174, 186, 205, 255))
        draw.text((x + 82, y + 50), "compatibility alias", fill=(104, 124, 184, 255))
    board.save(out_dir / "compat_alias_preview.png")

print(f"lpc assets generated: {len(entries)}")
print(f"lpc base tiles: {len(by_id)}")
print(f"lpc compatibility aliases: {alias_count}")
PY

echo "wrote ${OUT_DIR}/manifest.json and ${OUT_DIR}/tile_map.json"
