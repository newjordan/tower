#!/usr/bin/env bash
set -euo pipefail

SOURCE_URL="${SOURCE_URL:-https://opengameart.org/sites/default/files/Urban%20Character%20Pack%20small%20transparent_0.png}"
SOURCE_PATH="${SOURCE_PATH:-assets/public/v1/sources/urban_character_pack/urban_small_transparent.png}"
OUT_DIR="${OUT_DIR:-assets/public/v1/imported/urban_character_pack}"
CELL_SIZE="${CELL_SIZE:-40}"
DOWNLOAD_SOURCE="${DOWNLOAD_SOURCE:-1}"

mkdir -p "$(dirname "$SOURCE_PATH")"
mkdir -p "$OUT_DIR/tiles"

if [[ ! -f "$SOURCE_PATH" ]]; then
  if [[ "$DOWNLOAD_SOURCE" == "1" ]]; then
    echo "downloading source: $SOURCE_URL"
    curl -L --fail -sS "$SOURCE_URL" -o "$SOURCE_PATH"
  else
    echo "missing source file: $SOURCE_PATH"
    echo "set DOWNLOAD_SOURCE=1 or provide SOURCE_PATH"
    exit 1
  fi
fi

python3 - "$SOURCE_PATH" "$OUT_DIR" "$CELL_SIZE" "$SOURCE_URL" <<'PY'
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

from PIL import Image, ImageDraw

source_path = Path(sys.argv[1])
out_dir = Path(sys.argv[2])
cell_size = int(sys.argv[3])
source_url = sys.argv[4]

img = Image.open(source_path).convert("RGBA")
width, height = img.size
if width % cell_size != 0 or height % cell_size != 0:
    raise SystemExit(f"image size {width}x{height} not divisible by cell size {cell_size}")

tiles_dir = out_dir / "tiles"
tiles_dir.mkdir(parents=True, exist_ok=True)

cols = width // cell_size
rows = height // cell_size

entries = []
for row in range(rows):
    for col in range(cols):
        x0 = col * cell_size
        y0 = row * cell_size
        tile = img.crop((x0, y0, x0 + cell_size, y0 + cell_size))
        if tile.getchannel("A").getbbox() is None:
            continue
        asset_id = f"agent_urban_r{row}c{col}"
        out_path = tiles_dir / f"{asset_id}.png"
        tile.save(out_path)
        entries.append(
            {
                "asset_id": asset_id,
                "source_cell": {"row": row, "col": col},
                "source_rect": {"x": x0, "y": y0, "w": cell_size, "h": cell_size},
                "file": str(out_path).replace("\\", "/"),
                "tags": [
                    "agent",
                    "agent_sprite",
                    "urban",
                    "cc0",
                    f"row_{row}",
                    f"col_{col}",
                ],
            }
        )

by_id = {entry["asset_id"]: entry for entry in entries}
alias_map = {
    "agent_coder_idle": "agent_urban_r0c0",
    "agent_coder_work": "agent_urban_r0c1",
    "agent_coder_fail": "agent_urban_r0c2",
    "agent_worker_idle": "agent_urban_r0c3",
    "agent_worker_move": "agent_urban_r0c4",
    "agent_worker_alert": "agent_urban_r0c5",
}

aliased = 0
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
            "source_cell": base.get("source_cell"),
            "source_rect": base.get("source_rect"),
            "file": str(alias_path).replace("\\", "/"),
            "tags": ["agent", "agent_sprite", "urban", "cc0", "semantic_alias"],
        }
    )
    aliased += 1

manifest = {
    "pack_id": "urban_character_pack",
    "pack_name": "Urban Character Pack",
    "source_url": source_url,
    "source_file": str(source_path).replace("\\", "/"),
    "license": "CC0",
    "cell_size": cell_size,
    "image_size": {"width": width, "height": height},
    "grid_size": {"rows": rows, "cols": cols},
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

# Generate a quick alias review preview.
alias_assets = [a for a in entries if "semantic_alias" in a.get("tags", [])]
if alias_assets:
    card_w = 220
    card_h = 76
    cols = 2
    pad = 10
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
        draw.rectangle((x, y, x + card_w, y + card_h), fill=(32, 36, 45, 255), outline=(72, 158, 92, 255), width=2)
        tile = Image.open(entry["file"]).convert("RGBA").resize((56, 56), Image.NEAREST)
        board.alpha_composite(tile, (x + 8, y + 10))
        draw.text((x + 72, y + 14), entry["asset_id"], fill=(232, 238, 247, 255))
        draw.text((x + 72, y + 36), f"base: {entry.get('alias_of', '?')}", fill=(174, 186, 205, 255))
    board.save(out_dir / "compat_alias_preview.png")

print(f"urban assets generated: {len(entries)}")
print(f"urban base cells: {len(by_id)}")
print(f"urban compatibility aliases: {aliased}")
PY

echo "wrote ${OUT_DIR}/manifest.json and ${OUT_DIR}/tile_map.json"
