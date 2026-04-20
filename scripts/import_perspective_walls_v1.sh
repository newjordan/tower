#!/usr/bin/env bash
set -euo pipefail

SOURCE_URL="${SOURCE_URL:-https://opengameart.org/sites/default/files/perspective_walls.png}"
SOURCE_PATH="${SOURCE_PATH:-assets/public/v1/sources/perspective_walls_template/perspective_walls.png}"
OUT_DIR="${OUT_DIR:-assets/public/v1/imported/perspective_walls_template}"
CELL_SIZE="${CELL_SIZE:-32}"
TILE_SIZE="${TILE_SIZE:-64}"
INCLUDE_ROWS="${INCLUDE_ROWS:-0-6}"
DOWNLOAD_SOURCE="${DOWNLOAD_SOURCE:-1}"
GENERATE_SEMANTIC_ALIASES="${GENERATE_SEMANTIC_ALIASES:-1}"

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

python3 - "$SOURCE_PATH" "$OUT_DIR" "$CELL_SIZE" "$TILE_SIZE" "$INCLUDE_ROWS" "$SOURCE_URL" <<'PY'
import json
import os
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

from PIL import Image

source_path = Path(sys.argv[1])
out_dir = Path(sys.argv[2])
cell_size = int(sys.argv[3])
tile_size = int(sys.argv[4])
include_rows_raw = sys.argv[5]
source_url = sys.argv[6]

img = Image.open(source_path).convert("RGBA")
width, height = img.size
if width < tile_size or height < tile_size:
    raise SystemExit(f"image size {width}x{height} is smaller than tile size {tile_size}")
if cell_size <= 0 or tile_size <= 0:
    raise SystemExit(f"invalid sizes: cell_size={cell_size} tile_size={tile_size}")
if tile_size % cell_size != 0:
    raise SystemExit(f"tile size {tile_size} must be a multiple of cell size {cell_size}")

max_col = (width - tile_size) // cell_size + 1
max_row = (height - tile_size) // cell_size + 1

def parse_rows(expr: str) -> list[int]:
    if "-" in expr:
        a, b = expr.split("-", 1)
        start = int(a)
        end = int(b)
        return list(range(start, end + 1))
    return [int(x.strip()) for x in expr.split(",") if x.strip()]

rows = parse_rows(include_rows_raw)
for r in rows:
    if r < 0 or r >= max_row:
        raise SystemExit(f"row {r} out of range 0..{max_row-1}")

tiles_dir = out_dir / "tiles"
tiles_dir.mkdir(parents=True, exist_ok=True)

entries = []
for row in rows:
    for col in range(max_col):
        asset_id = f"tile_wall_r{row}c{col}"
        x0 = col * cell_size
        y0 = row * cell_size
        tile = img.crop((x0, y0, x0 + tile_size, y0 + tile_size))
        out_path = tiles_dir / f"{asset_id}.png"
        tile.save(out_path)
        entries.append(
            {
                "asset_id": asset_id,
                "anchor": {"x": 0, "y": 0, "step": cell_size},
                "source_cell": {"row": row, "col": col},
                "source_rect": {"x": x0, "y": y0, "w": tile_size, "h": tile_size},
                "file": str(out_path).replace("\\", "/"),
                "tags": ["tile", "wall", "template", f"row_{row}", f"col_{col}"],
            }
        )

# Keep compatibility with existing tile id while new IDs are adopted.
aliases = {
    "tile_wall_terminal": "tile_wall_r2c4",
    "tile_wall_primary": "tile_wall_r0c0",
}
for alias_id, base_id in aliases.items():
    base = next((e for e in entries if e["asset_id"] == base_id), None)
    if base is None:
        continue
    alias_path = tiles_dir / f"{alias_id}.png"
    shutil.copyfile(base["file"], alias_path)
    entries.append(
        {
            "asset_id": alias_id,
            "alias_of": base_id,
            "anchor": {"x": 0, "y": 0, "step": cell_size},
            "source_cell": base["source_cell"],
            "source_rect": base["source_rect"],
            "file": str(alias_path).replace("\\", "/"),
            "tags": ["tile", "wall", "alias"],
        }
    )

manifest = {
    "pack_id": "perspective_walls_template",
    "pack_name": "Perspective Walls Template",
    "source_url": source_url,
    "source_file": str(source_path).replace("\\", "/"),
    "license": "CC0",
    "cell_size": cell_size,
    "tile_size": tile_size,
    "image_size": {"width": width, "height": height},
    "included_rows": rows,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "assets": entries,
}

with (out_dir / "tile_map.json").open("w", encoding="utf-8") as fh:
    json.dump({"assets": entries}, fh, indent=2)
    fh.write("\n")

with (out_dir / "manifest.json").open("w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")

print(f"generated {len(entries)} wall tiles into {tiles_dir}")
PY

echo "wrote ${OUT_DIR}/manifest.json and ${OUT_DIR}/tile_map.json"

if [[ "$GENERATE_SEMANTIC_ALIASES" == "1" ]]; then
  if [[ -x "scripts/alias_perspective_walls_semantic_v1.sh" ]]; then
    ROOT_DIR="$OUT_DIR" scripts/alias_perspective_walls_semantic_v1.sh
  else
    echo "semantic alias script not found: scripts/alias_perspective_walls_semantic_v1.sh"
  fi
fi
