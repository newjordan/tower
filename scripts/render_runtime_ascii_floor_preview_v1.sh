#!/usr/bin/env bash
set -euo pipefail

FLOOR_SELECTOR="${1:-all}"
OUTPUT_DIR="${2:-assets/previews/runtime}"

if ! [[ "$FLOOR_SELECTOR" =~ ^all$|^[0-9]+$ ]]; then
  echo "usage: scripts/render_runtime_ascii_floor_preview_v1.sh [all|floor_index>=0] [output_dir]" >&2
  exit 2
fi

SERVER_LOG="${TOWER_RENDER_ASCII_PREVIEW_SERVER_LOG:-/tmp/tower-runtime-ascii-preview-server.log}"
BOOT_TIMEOUT_S="${TOWER_RENDER_ASCII_PREVIEW_BOOT_TIMEOUT_S:-120}"

cargo run >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
cleanup() {
  kill "$SERVER_PID" >/dev/null 2>&1 || true
  wait "$SERVER_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 "$BOOT_TIMEOUT_S"); do
  if rg -q "server listening on 0.0.0.0:3000" "$SERVER_LOG"; then
    ready=1
    break
  fi
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  echo "server_not_ready" >&2
  tail -n 120 "$SERVER_LOG" >&2 || true
  exit 2
fi

bootstrap_ready=0
for _ in $(seq 1 30); do
  if curl -sS --fail http://127.0.0.1:3000/tower/tower-1/bootstrap > /tmp/tower-bootstrap-ascii-preview.json; then
    if [ -s /tmp/tower-bootstrap-ascii-preview.json ]; then
      bootstrap_ready=1
      break
    fi
  fi
  sleep 1
done
if [ "$bootstrap_ready" -ne 1 ]; then
  echo "bootstrap_not_ready" >&2
  tail -n 120 "$SERVER_LOG" >&2 || true
  exit 2
fi

python3 - "$FLOOR_SELECTOR" "$OUTPUT_DIR" <<'PY'
import json
import math
import re
import sys
from collections import defaultdict
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

floor_selector = sys.argv[1]
output_dir = Path(sys.argv[2])

boot = json.loads(Path('/tmp/tower-bootstrap-ascii-preview.json').read_text())
payload = boot['snapshot']['payload']
floors = payload.get('floors', [])
placements = payload.get('runtime_floor_placements', {}).get('floors', [])
runtime_assets = payload.get('runtime_assets', {})

if not floors:
    raise SystemExit('no floors in bootstrap payload')

placement_by_floor_id = {
    p.get('floor_id'): p for p in placements if isinstance(p, dict) and isinstance(p.get('floor_id'), str)
}

def tile_token(tile_id):
    if tile_id == 'tile_floor_clean':
        return ' '
    if tile_id == 'tile_wall_corner_top_left':
        return '⡏'
    if tile_id == 'tile_wall_corner_top_right':
        return '⢹'
    if tile_id == 'tile_wall_corner_bottom_left':
        return '⣇'
    if tile_id == 'tile_wall_corner_bottom_right':
        return '⣸'
    if tile_id == 'tile_wall_inner_corner_top_left':
        return '⢹'
    if tile_id == 'tile_wall_inner_corner_top_right':
        return '⡏'
    if tile_id == 'tile_wall_inner_corner_bottom_left':
        return '⣸'
    if tile_id == 'tile_wall_inner_corner_bottom_right':
        return '⣇'
    if tile_id == 'tile_wall_edge_top':
        return '⠉'
    if tile_id == 'tile_wall_edge_bottom':
        return '⠤'
    if tile_id == 'tile_wall_edge_left':
        return '⡇'
    if tile_id == 'tile_wall_edge_right':
        return '⢸'
    if tile_id == 'tile_wall_door_left':
        return '⠪'
    if tile_id == 'tile_wall_door_right':
        return '⠕'
    if tile_id == 'tile_wall_window_left':
        return '⡆'
    if tile_id == 'tile_wall_window_right':
        return '⢃'
    if tile_id == 'tile_wall_cross':
        return '⢼'
    if tile_id == 'tile_wall_terminal':
        return '⣾'
    if tile_id in ('tile_wall_pillar', 'tile_wall_pillar_alt'):
        return '⣿'
    if tile_id == 'tile_wall_t_top':
        return '⢸'
    if tile_id == 'tile_wall_t_bottom':
        return '⡇'
    if isinstance(tile_id, str) and tile_id.startswith('tile_wall_t_'):
        return '⢼'
    if isinstance(tile_id, str) and tile_id.startswith('tile_wall_'):
        return '⣿'
    return '?'

def infer_entity_kind(asset_id):
    if not isinstance(asset_id, str):
        return ''
    if asset_id.startswith('agent_'):
        return 'agent'
    if asset_id.startswith('prop_'):
        return 'prop'
    if asset_id.startswith('fx_'):
        return 'fx'
    if asset_id.startswith('ui_'):
        return 'ui'
    if asset_id.startswith('portrait_'):
        return 'portrait'
    return ''

def entity_token(asset_id, kind=''):
    resolved_kind = kind or infer_entity_kind(asset_id)
    if isinstance(asset_id, str):
        if asset_id.startswith('agent_'):
            if 'alert' in asset_id or 'fail' in asset_id:
                return '!'
            if 'move' in asset_id:
                return '&'
            return '@'
        if asset_id.startswith('prop_'):
            if 'desk' in asset_id:
                return '='
            if 'planning' in asset_id:
                return 'B'
            return '+'
        if asset_id.startswith('fx_'):
            return '*'
        if asset_id.startswith('ui_'):
            return 'U'
        if asset_id.startswith('portrait_'):
            return ':'
        if asset_id.startswith('tile_'):
            return tile_token(asset_id)
    if resolved_kind == 'agent':
        return '@'
    if resolved_kind == 'prop':
        return '='
    if resolved_kind == 'fx':
        return '*'
    if resolved_kind == 'ui':
        return 'U'
    return '?'

def safe_floor_slug(floor):
    floor_id = floor.get('id') if isinstance(floor.get('id'), str) else 'floor'
    floor_name = floor.get('name') if isinstance(floor.get('name'), str) else floor_id
    raw = f"{floor_id}_{floor_name}".lower()
    slug = re.sub(r'[^a-z0-9]+', '_', raw).strip('_')
    return slug or 'floor'

def load_font(path, size):
    try:
        return ImageFont.truetype(path, size=size)
    except Exception:
        return ImageFont.load_default()

def draw_outlined_text(draw, xy, text, font, fill=(255, 255, 255, 255), outline=(0, 0, 0, 255)):
    x, y = xy
    # thin black outline for overlap readability
    draw.text((x - 1, y), text, font=font, fill=outline)
    draw.text((x + 1, y), text, font=font, fill=outline)
    draw.text((x, y - 1), text, font=font, fill=outline)
    draw.text((x, y + 1), text, font=font, fill=outline)
    draw.text((x, y), text, font=font, fill=fill)

def text_center(draw, cell_left, cell_top, cell_size, text, font):
    bbox = draw.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    cx = cell_left + (cell_size - tw) // 2
    cy = cell_top + (cell_size - th) // 2 - 1
    return cx, cy

agent_assets = {}
for entry in runtime_assets.get('agents') or []:
    if not isinstance(entry, dict):
        continue
    eid = entry.get('entity_id')
    aid = entry.get('asset_id')
    if isinstance(eid, str) and isinstance(aid, str):
        agent_assets[eid] = aid

if floor_selector == 'all':
    indices = list(range(len(floors)))
else:
    idx = int(floor_selector)
    if idx < 0 or idx >= len(floors):
        raise SystemExit(f'floor_index out of range: {idx} >= {len(floors)}')
    indices = [idx]

output_dir.mkdir(parents=True, exist_ok=True)

font_path = '/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf'
cell_font = load_font(font_path, 18)
header_font = load_font(font_path, 17)
meta_font = load_font(font_path, 13)
legend_font = load_font(font_path, 12)
badge_font = load_font(font_path, 10)

tile_size = 28
padding = 12
header_h = 64
footer_h = 40

written = []
for floor_index in indices:
    floor = floors[floor_index]
    floor_id = floor.get('id')
    placement = placement_by_floor_id.get(floor_id, {'props': [], 'agents': [], 'fx': []})

    tilemap = floor.get('tilemap', {})
    width = int(tilemap.get('width', 1))
    height = int(tilemap.get('height', 1))
    tiles = tilemap.get('tiles', [])
    if width <= 0:
        width = 1
    if height <= 0:
        height = 1
    expected = width * height
    if len(tiles) < expected:
        tiles = list(tiles) + ['tile_floor_clean'] * (expected - len(tiles))
    elif len(tiles) > expected:
        tiles = list(tiles[:expected])

    img_w = padding * 2 + width * tile_size
    img_h = header_h + padding + height * tile_size + footer_h

    img = Image.new('RGBA', (img_w, img_h), (0, 0, 0, 255))
    draw = ImageDraw.Draw(img)

    # header
    draw.rectangle([0, 0, img_w - 1, header_h], fill=(3, 3, 3, 255))
    title = f"ASCII Floorplan v1 - {floor.get('name', floor_id)}"
    meta = f"floor={floor_id}  idx={floor_index}  tick={payload.get('tick', 0)}  size={width}x{height}"
    draw.text((padding, 10), title, font=header_font, fill=(255, 255, 255, 255))
    draw.text((padding, 34), meta, font=meta_font, fill=(186, 186, 186, 255))

    # entity placement map
    by_cell = defaultdict(list)
    for kind in ('props', 'agents', 'fx'):
        entries = placement.get(kind) or []
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            x = entry.get('x')
            y = entry.get('y')
            if not isinstance(x, int) or not isinstance(y, int):
                continue
            if x < 0 or y < 0 or x >= width or y >= height:
                continue
            entity_id = entry.get('entity_id') if isinstance(entry.get('entity_id'), str) else ''
            asset_id = entry.get('asset_id') if isinstance(entry.get('asset_id'), str) else ''
            if kind == 'agents' and entity_id:
                asset_id = agent_assets.get(entity_id, asset_id)
            by_cell[(x, y)].append({
                'kind': 'agent' if kind == 'agents' else ('prop' if kind == 'props' else 'fx'),
                'asset_id': asset_id,
            })

    def entity_rank(kind):
        if kind == 'prop':
            return 1
        if kind == 'fx':
            return 2
        if kind == 'agent':
            return 3
        return 0

    # grid cells + tile glyphs
    for idx, tile_id in enumerate(tiles):
        x = idx % width
        y = idx // width
        left = padding + x * tile_size
        top = header_h + y * tile_size
        right = left + tile_size - 1
        bottom = top + tile_size - 1
        draw.rectangle([left, top, right, bottom], fill=(0, 0, 0, 255), outline=(10, 10, 10, 255))
        tok = tile_token(tile_id)
        tx, ty = text_center(draw, left, top, tile_size, tok, cell_font)
        draw_outlined_text(draw, (tx, ty), tok, cell_font)

    # entity overlays
    for (x, y), entities in by_cell.items():
        left = padding + x * tile_size
        top = header_h + y * tile_size
        layered = sorted(entities, key=lambda e: entity_rank(e.get('kind', '')))
        primary = layered[-1]
        tok = entity_token(primary.get('asset_id', ''), primary.get('kind', ''))
        tx, ty = text_center(draw, left, top, tile_size, tok, cell_font)
        draw_outlined_text(draw, (tx, ty), tok, cell_font)

        if len(entities) > 1:
            badge = f"+{len(entities) - 1}"
            bbox = draw.textbbox((0, 0), badge, font=badge_font)
            bw = bbox[2] - bbox[0]
            bh = bbox[3] - bbox[1]
            bx = left + tile_size - bw - 3
            by = top + tile_size - bh - 2
            draw.rectangle([bx - 1, by - 1, bx + bw + 1, by + bh], fill=(0, 0, 0, 255), outline=(32, 32, 32, 255))
            draw_outlined_text(draw, (bx, by - 1), badge, badge_font)

    # footer legend
    legend_top = header_h + height * tile_size + 8
    legend = "legend: walls ⡏⢹⣇⣸⠉⠤⡇⢸  desks =  workers @ ! &  fx *  floor blank  +N stacked"
    draw.text((padding, legend_top), legend, font=legend_font, fill=(176, 176, 176, 255))

    slug = safe_floor_slug(floor)
    out = output_dir / f"tower_runtime_ascii_floorplan_v1_{floor_index}_{slug}.png"
    img.save(out)
    written.append(str(out))

for path in written:
    print(path)
PY
