#!/usr/bin/env python3
"""
Build tile_map.json and manifest.json from the curated tile set.
No extraction from source PNG — tiles are pre-identified and named by hand.
Creates semantic aliases by copying curated tiles to sim.rs semantic IDs.
"""
import json
import shutil
from datetime import datetime, timezone
from pathlib import Path

TILES_DIR = Path("assets/public/v1/imported/perspective_walls_template/tiles")
OUT_DIR = TILES_DIR.parent

# Curated tiles → semantic ID mapping
# Each entry: (curated_filename_stem, [list of semantic alias IDs])
SEMANTIC_MAP = [
    # Edges (primary walls)
    ("tile_wall_primary_lat",       ["tile_wall_edge_top", "tile_wall_edge_bottom"]),
    ("tile_wall_primary_alt_vert",  ["tile_wall_edge_left", "tile_wall_edge_right"]),
    # Outer corners
    ("tile_wall_corner_top_left",     ["tile_wall_corner_top_left"]),
    ("tile_wall_corner_top_right",    ["tile_wall_corner_top_right"]),
    ("tile_wall_corner_bottom_left",  ["tile_wall_corner_bottom_left"]),
    ("tile_wall_corner_bottom_right", ["tile_wall_corner_bottom_right"]),
    # T-junctions
    ("tile_wall_T_top",     ["tile_wall_t_top"]),
    ("tile_wall_t_bot",     ["tile_wall_t_bottom"]),
    ("tile_wall_T_left",    ["tile_wall_t_left"]),
    ("tile_wall_T_right",   ["tile_wall_t_right"]),
    # Cross / center
    ("tile_wall_T_center",  ["tile_wall_cross"]),
    # Pillar
    ("tile_wall_pillar_simple", ["tile_wall_pillar", "tile_wall_pillar_alt"]),
    # Doors — lateral (horizontal wall) and vertical (side wall)
    ("tile_door_lat",   ["tile_wall_door_left", "tile_wall_door_right"]),
    ("tile_door_vert",  ["tile_wall_door_vert"]),
    # Terminal (reuse pillar — dead-end cap)
    ("tile_wall_pillar_simple", ["tile_wall_terminal"]),
    # Windows (reuse primary wall — no dedicated window tile in curated set)
    ("tile_wall_primary_alt_vert", ["tile_wall_window_left", "tile_wall_window_right"]),
    # Floor
    ("floor_tile", ["tile_floor_clean"]),
    # Props
    ("table_basic", ["tile_prop_table"]),
]

# Also map inner corners to the regular corners as fallback
SEMANTIC_MAP += [
    ("tile_wall_corner_top_left",     ["tile_wall_inner_corner_top_left"]),
    ("tile_wall_corner_top_right",    ["tile_wall_inner_corner_top_right"]),
    ("tile_wall_corner_bottom_left",  ["tile_wall_inner_corner_bottom_left"]),
    ("tile_wall_corner_bottom_right", ["tile_wall_inner_corner_bottom_right"]),
]

# Build base asset entries from all PNGs in tiles dir (excluding subdirs)
entries = []
for png in sorted(TILES_DIR.glob("*.png")):
    asset_id = png.stem
    entries.append({
        "asset_id": asset_id,
        "anchor": {"x": 0, "y": 0, "step": 32},
        "file": str(png).replace("\\", "/"),
        "tags": ["tile", "curated"],
    })

base_by_id = {e["asset_id"]: e for e in entries}

# Create semantic aliases
aliases_created = 0
for base_stem, semantic_ids in SEMANTIC_MAP:
    base = base_by_id.get(base_stem)
    if base is None:
        print(f"WARNING: curated tile not found: {base_stem}")
        continue
    for sem_id in semantic_ids:
        is_existing_base = sem_id in base_by_id
        dst = TILES_DIR / f"{sem_id}.png"
        src = Path(base["file"])
        # Always copy to keep alias in sync with source
        try:
            if dst.exists() and dst.samefile(src):
                pass  # already the right file
            else:
                shutil.copyfile(src, dst)
        except (OSError, shutil.SameFileError):
            pass  # already exists
        if is_existing_base:
            continue  # already has a base entry, don't add duplicate
        entries.append({
            "asset_id": sem_id,
            "alias_of": base_stem,
            "anchor": {"x": 0, "y": 0, "step": 32},
            "file": str(dst).replace("\\", "/"),
            "tags": ["tile", "wall", "semantic_alias", "curated"],
        })
        aliases_created += 1

# Write tile_map.json
tile_map = {"assets": entries}
with (OUT_DIR / "tile_map.json").open("w", encoding="utf-8") as fh:
    json.dump(tile_map, fh, indent=2)
    fh.write("\n")

# Write manifest.json
manifest = {
    "pack_id": "perspective_walls_template",
    "pack_name": "Perspective Walls Template",
    "source_url": "https://opengameart.org/sites/default/files/perspective_walls.png",
    "license": "CC0",
    "cell_size": 32,
    "tile_size": 64,
    "note": "Curated tile set — tiles hand-identified and named from source sheet",
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "assets": entries,
}
with (OUT_DIR / "manifest.json").open("w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")

print(f"base tiles: {len(base_by_id)}")
print(f"semantic aliases created: {aliases_created}")
print(f"total entries: {len(entries)}")
