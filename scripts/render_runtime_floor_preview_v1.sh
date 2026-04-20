#!/usr/bin/env bash
set -euo pipefail

FLOOR_INDEX="${1:-0}"
OUTPUT_PATH="${2:-/tmp/tower_runtime_floor_scene_v1.png}"
if ! [[ "$FLOOR_INDEX" =~ ^[0-9]+$ ]]; then
  echo "usage: scripts/render_runtime_floor_preview_v1.sh [floor_index>=0] [output_path]" >&2
  exit 2
fi

SERVER_LOG="${TOWER_RENDER_PREVIEW_SERVER_LOG:-/tmp/tower-runtime-scene-server.log}"
BOOT_TIMEOUT_S="${TOWER_RENDER_PREVIEW_BOOT_TIMEOUT_S:-120}"

cargo run >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
cleanup() {
  kill "$SERVER_PID" >/dev/null 2>&1 || true
  wait "$SERVER_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 "$BOOT_TIMEOUT_S"); do
  if grep -q "server listening on 0.0.0.0:3000" "$SERVER_LOG" 2>/dev/null; then
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

curl -sS http://127.0.0.1:3000/tower/tower-1/bootstrap > /tmp/tower-bootstrap.json
curl -sS http://127.0.0.1:3000/assets/public/v1/catalog > /tmp/tower-catalog.json

python3 - "$FLOOR_INDEX" "$OUTPUT_PATH" <<'PY'
import json
import sys
from io import BytesIO
from pathlib import Path
from urllib.parse import urljoin
from urllib.request import urlopen

from PIL import Image, ImageDraw, ImageFont

floor_index = int(sys.argv[1])
out_path = Path(sys.argv[2])

boot = json.loads(Path('/tmp/tower-bootstrap.json').read_text())
catalog = json.loads(Path('/tmp/tower-catalog.json').read_text())

payload = boot['snapshot']['payload']
floors = payload.get('floors', [])
placements = payload.get('runtime_floor_placements', {}).get('floors', [])
runtime_assets = payload.get('runtime_assets', {})

if not floors:
    raise SystemExit('no floors in bootstrap payload')
if floor_index >= len(floors):
    raise SystemExit(f'floor_index out of range: {floor_index} >= {len(floors)}')

floor = floors[floor_index]
placement = next((f for f in placements if f.get('floor_id') == floor.get('id')), None)
if placement is None:
    placement = {'props': [], 'agents': [], 'fx': []}

tilemap = floor.get('tilemap', {})
width = int(tilemap.get('width', 1))
height = int(tilemap.get('height', 1))
tiles = tilemap.get('tiles', [])
tile_size = 32
wall_overhang = 32
map_origin_y = 70 + wall_overhang

asset_url_by_id = {}
for aid, entry in (catalog.get('tiles') or {}).items():
    url = entry.get('url') if isinstance(entry, dict) else None
    if isinstance(url, str):
        asset_url_by_id[aid] = url

def fetch_asset_image(asset_id):
    rel = asset_url_by_id.get(asset_id)
    if not rel:
        return None
    url = urljoin('http://127.0.0.1:3000', rel)
    try:
        with urlopen(url, timeout=10) as r:
            raw = r.read()
        return Image.open(BytesIO(raw)).convert('RGBA')
    except Exception:
        return None

cache = {}
def load_asset(asset_id):
    if asset_id in cache:
        return cache[asset_id]
    img = fetch_asset_image(asset_id)
    cache[asset_id] = img
    return img

canvas = Image.new(
    'RGBA',
    (
        width * tile_size + wall_overhang,
        map_origin_y + height * tile_size + wall_overhang,
    ),
    (12, 16, 24, 255),
)
draw = ImageDraw.Draw(canvas)
font = ImageFont.load_default()

floor_asset = load_asset('tile_floor_clean')

# Pass 1: Draw floor everywhere
for idx in range(width * height):
    x = (idx % width) * tile_size
    y = (idx // width) * tile_size + map_origin_y
    if floor_asset:
        canvas.paste(floor_asset, (x, y))
    else:
        draw.rectangle([x, y, x + tile_size - 1, y + tile_size - 1], fill=(128, 128, 128, 255))

# Pass 2: Draw tiles (walls, etc.) on top
for idx, tile_id in enumerate(tiles):
    if tile_id == 'tile_floor_clean':
        continue
    x = (idx % width) * tile_size
    y = (idx // width) * tile_size + map_origin_y
    tile = load_asset(tile_id)
    if tile is None:
        draw.rectangle([x, y, x + tile_size - 1, y + tile_size - 1], fill=(26, 34, 52, 255), outline=(48, 62, 90, 255))
        continue
    # Perspective walls are authored as 64x64 sprites on a 32px base grid.
    if tile.width > tile_size or tile.height > tile_size:
        sprite_x = x
        sprite_y = y - wall_overhang
    else:
        sprite_x = x
        sprite_y = y
    canvas.paste(tile, (sprite_x, sprite_y), tile)

agent_assets = {}
for entry in runtime_assets.get('agents') or []:
    eid = entry.get('entity_id')
    aid = entry.get('asset_id')
    if isinstance(eid, str) and isinstance(aid, str):
        agent_assets[eid] = aid

for kind in ('props', 'agents', 'fx'):
    for entry in placement.get(kind) or []:
        ex = entry.get('x')
        ey = entry.get('y')
        if not isinstance(ex, int) or not isinstance(ey, int):
            continue
        if ex < 0 or ey < 0 or ex >= width or ey >= height:
            continue
        asset_id = entry.get('asset_id')
        if kind == 'agents':
            entity_id = entry.get('entity_id')
            if isinstance(entity_id, str):
                asset_id = agent_assets.get(entity_id, asset_id)
        if not isinstance(asset_id, str):
            continue
        sprite = load_asset(asset_id)
        if sprite is None:
            continue
        sprite = sprite.resize((tile_size, tile_size), Image.Resampling.NEAREST)
        px = ex * tile_size
        py = ey * tile_size + map_origin_y
        canvas.paste(sprite, (px, py), sprite)

draw.rectangle([0, 0, canvas.width, 70], fill=(18, 26, 40, 255))
draw.text((10, 10), f"Tower Runtime Floor Preview v1 - {floor.get('name', 'floor')}", fill=(226, 238, 255, 255), font=font)
draw.text((10, 30), f"size: {width}x{height}  tick: {payload.get('tick', 0)}", fill=(166, 192, 222, 255), font=font)
draw.text((10, 46), "tiles + runtime prop/agent/fx overlays from bootstrap/catalog", fill=(146, 174, 204, 255), font=font)

out_path.parent.mkdir(parents=True, exist_ok=True)
canvas.save(out_path)
print(str(out_path))
PY
