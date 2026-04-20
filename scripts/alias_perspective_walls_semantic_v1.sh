#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${ROOT_DIR:-assets/public/v1/imported/perspective_walls_template}"
TILES_DIR="${TILES_DIR:-$ROOT_DIR/tiles}"
TILE_MAP_PATH="${TILE_MAP_PATH:-$ROOT_DIR/tile_map.json}"
MANIFEST_PATH="${MANIFEST_PATH:-$ROOT_DIR/manifest.json}"
PREVIEW_PATH="${PREVIEW_PATH:-$ROOT_DIR/semantic_alias_preview.png}"
REVIEW_MD_PATH="${REVIEW_MD_PATH:-$ROOT_DIR/semantic_alias_review.md}"
ROLE_MAP_PATH="${ROLE_MAP_PATH:-scripts/perspective_walls_role_map_v1.json}"

if [[ ! -f "$TILE_MAP_PATH" ]]; then
  echo "missing tile map: $TILE_MAP_PATH"
  exit 1
fi
if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "missing manifest: $MANIFEST_PATH"
  exit 1
fi
if [[ ! -f "$ROLE_MAP_PATH" ]]; then
  echo "missing role map: $ROLE_MAP_PATH"
  exit 1
fi

python3 - "$TILES_DIR" "$TILE_MAP_PATH" "$MANIFEST_PATH" "$PREVIEW_PATH" "$REVIEW_MD_PATH" "$ROLE_MAP_PATH" <<'PY'
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

from PIL import Image, ImageDraw

tiles_dir = Path(sys.argv[1])
tile_map_path = Path(sys.argv[2])
manifest_path = Path(sys.argv[3])
preview_path = Path(sys.argv[4])
review_md_path = Path(sys.argv[5])
role_map_path = Path(sys.argv[6])

with tile_map_path.open("r", encoding="utf-8") as fh:
    tile_map = json.load(fh)
with manifest_path.open("r", encoding="utf-8") as fh:
    manifest = json.load(fh)
with role_map_path.open("r", encoding="utf-8") as fh:
    role_map = json.load(fh)

assets = tile_map.get("assets", [])
base_entries = {
    a["asset_id"]: a
    for a in assets
    if a.get("asset_id", "").startswith("tile_wall_r")
}

for need in ("edge_top", "edge_bottom", "edge_left", "edge_right",
             "corner_top_left", "corner_top_right", "corner_bottom_left", "corner_bottom_right"):
    bid = role_map.get(need)
    if bid not in base_entries:
        raise SystemExit(f"role map references missing base tile: {need}={bid}")

# Deterministic semantic mapping — all entries are fixed lookups from the role map.
semantic_defs = [
    ("tile_wall_edge_top", role_map["edge_top"], "edge", "fixed_lookup_v2"),
    ("tile_wall_edge_bottom", role_map["edge_bottom"], "edge", "fixed_lookup_v2"),
    ("tile_wall_edge_left", role_map["edge_left"], "edge", "fixed_lookup_v2"),
    ("tile_wall_edge_right", role_map["edge_right"], "edge", "fixed_lookup_v2"),
    ("tile_wall_corner_top_left", role_map["corner_top_left"], "corner", "fixed_lookup_v2"),
    ("tile_wall_corner_top_right", role_map["corner_top_right"], "corner", "fixed_lookup_v2"),
    ("tile_wall_corner_bottom_left", role_map["corner_bottom_left"], "corner", "fixed_lookup_v2"),
    ("tile_wall_corner_bottom_right", role_map["corner_bottom_right"], "corner", "fixed_lookup_v2"),
    ("tile_wall_inner_corner_top_left", role_map["inner_corner_top_left"], "inner_corner", "fixed_lookup_v2"),
    ("tile_wall_inner_corner_top_right", role_map["inner_corner_top_right"], "inner_corner", "fixed_lookup_v2"),
    ("tile_wall_inner_corner_bottom_left", role_map["inner_corner_bottom_left"], "inner_corner", "fixed_lookup_v2"),
    ("tile_wall_inner_corner_bottom_right", role_map["inner_corner_bottom_right"], "inner_corner", "fixed_lookup_v2"),
    ("tile_wall_t_top", role_map["t_top"], "junction", "fixed_lookup_v2"),
    ("tile_wall_t_bottom", role_map["t_bottom"], "junction", "fixed_lookup_v2"),
    ("tile_wall_t_left", role_map["t_left"], "junction", "fixed_lookup_v2"),
    ("tile_wall_t_right", role_map["t_right"], "junction", "fixed_lookup_v2"),
    ("tile_wall_cross", role_map["cross"], "junction", "fixed_lookup_v2"),
    ("tile_wall_terminal", role_map["terminal"], "feature", "fixed_lookup_v2"),
    ("tile_wall_pillar", role_map["pillar"], "feature", "fixed_lookup_v2"),
    ("tile_wall_pillar_alt", role_map["pillar_alt"], "feature", "fixed_lookup_v2"),
    ("tile_wall_door_left", role_map["door_left"], "door", "fixed_lookup_v2"),
    ("tile_wall_door_right", role_map["door_right"], "door", "fixed_lookup_v2"),
    ("tile_wall_window_left", role_map["window_left"], "window", "fixed_lookup_v2"),
    ("tile_wall_window_right", role_map["window_right"], "window", "fixed_lookup_v2"),
]

semantic_aliases = []
for aid, base_id, group, mode in semantic_defs:
    semantic_aliases.append(
        {
            "asset_id": aid,
            "base_id": base_id,
            "group": group,
            "confidence": "deterministic",
            "rationale": mode,
        }
    )

alias_ids = {a["asset_id"] for a in semantic_aliases}
filtered_assets = []
for entry in assets:
    aid = entry.get("asset_id")
    tags = entry.get("tags", [])
    if aid in alias_ids:
        continue
    if "semantic_alias" in tags:
        continue
    filtered_assets.append(entry)

added = 0
for alias in semantic_aliases:
    aid = alias["asset_id"]
    base = base_entries.get(alias["base_id"])
    if base is None:
        continue
    src = Path(base["file"])
    dst = tiles_dir / f"{aid}.png"
    shutil.copyfile(src, dst)
    filtered_assets.append(
        {
            "asset_id": aid,
            "alias_of": alias["base_id"],
            "semantic_group": alias["group"],
            "semantic_confidence": alias["confidence"],
            "semantic_rationale": alias["rationale"],
            "source_cell": base.get("source_cell"),
            "source_rect": base.get("source_rect"),
            "anchor": base.get("anchor"),
            "file": str(dst).replace("\\", "/"),
            "tags": ["tile", "wall", "semantic_alias", alias["group"], "deterministic"],
        }
    )
    added += 1

tile_map["assets"] = filtered_assets
with tile_map_path.open("w", encoding="utf-8") as fh:
    json.dump(tile_map, fh, indent=2)
    fh.write("\n")

manifest["assets"] = filtered_assets
manifest["semantic_aliases"] = semantic_aliases
manifest["semantic_aliases_generated_at"] = datetime.now(timezone.utc).isoformat()
manifest["semantic_solver"] = {
    "method": "fixed_lookup_v2",
    "deterministic": True,
    "role_map": str(role_map_path),
}
with manifest_path.open("w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")

aliases_for_preview = [a for a in filtered_assets if "semantic_alias" in a.get("tags", [])]
aliases_for_preview.sort(key=lambda a: (a.get("semantic_group", ""), a.get("asset_id", "")))
if aliases_for_preview:
    cols = 2
    card_w = 420
    card_h = 82
    pad = 12
    rows = (len(aliases_for_preview) + cols - 1) // cols
    board = Image.new(
        "RGBA",
        (cols * card_w + (cols + 1) * pad, rows * card_h + (rows + 1) * pad),
        (18, 21, 27, 255),
    )
    draw = ImageDraw.Draw(board)
    for idx, entry in enumerate(aliases_for_preview):
        col = idx % cols
        row = idx // cols
        x = pad + col * (card_w + pad)
        y = pad + row * (card_h + pad)
        draw.rectangle((x, y, x + card_w, y + card_h), fill=(32, 36, 45, 255), outline=(103, 163, 232, 255), width=2)
        tile = Image.open(entry["file"]).convert("RGBA").resize((64, 64), Image.NEAREST)
        board.alpha_composite(tile, (x + 8, y + 8))
        draw.text((x + 82, y + 10), entry["asset_id"], fill=(232, 238, 247, 255))
        draw.text((x + 82, y + 33), f"base: {entry.get('alias_of', '?')}", fill=(174, 186, 205, 255))
        draw.text((x + 82, y + 54), entry.get("semantic_rationale", ""), fill=(103, 163, 232, 255))
    board.save(preview_path)

lines = [
    "# Perspective Walls Semantic Alias Review",
    "",
    f"Generated at: {datetime.now(timezone.utc).isoformat()}",
    "",
    "| Asset ID | Base ID | Group | Mode |",
    "|---|---|---|---|",
]
for alias in semantic_aliases:
    lines.append(
        f"| {alias['asset_id']} | {alias['base_id']} | {alias['group']} | {alias['rationale']} |"
    )
review_md_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

print(f"semantic aliases written: {added}")
print(f"preview: {preview_path}")
print(f"review: {review_md_path}")
PY
