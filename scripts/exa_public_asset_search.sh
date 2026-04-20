#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${EXA_API_KEY:-}" ]]; then
  echo "EXA_API_KEY is required"
  exit 1
fi

OUT_DIR="${1:-research/exa}"
mkdir -p "$OUT_DIR"
ts="$(date -u +%Y%m%dT%H%M%SZ)"

run() {
  local name="$1"
  local query="$2"
  shift 2
  local domains_json payload
  domains_json="$(printf '%s\n' "$@" | jq -R . | jq -s .)"
  payload="$(jq -n \
    --arg q "$query" \
    --argjson d "$domains_json" \
    '{query:$q, numResults:12, includeDomains:$d, type:"auto"}')"
  curl -sS https://api.exa.ai/search \
    -H 'content-type: application/json' \
    -H "x-api-key: ${EXA_API_KEY}" \
    --data "$payload" > "${OUT_DIR}/${ts}_${name}.json"
  echo "saved ${OUT_DIR}/${ts}_${name}.json"
}

run public_sprite_packs \
  "best CC0 or permissive licensed 8-bit Atari-style sprite packs for commercial game prototypes" \
  opengameart.org kenney.nl itch.io

run pixel_ui_icon_packs \
  "free pixel UI icon packs 8-bit game assets permissive license" \
  opengameart.org itch.io kenney.nl

run office_tech_tilesets \
  "pixel art office or technology tileset free game assets" \
  opengameart.org itch.io

run palette_ready_assets \
  "16-color pixel sprite packs suitable for palette-constrained games" \
  opengameart.org itch.io github.com

run license_sources \
  "official license pages for kenney assets opengameart licensing and itch.io asset pack licenses" \
  kenney.nl opengameart.org itch.io

run atari_style_sprites \
  "Atari 2600 style sprite pack 8x8 16x16 free game assets" \
  opengameart.org itch.io github.com

run low_color_palette_packs \
  "4-color or 8-color pixel sprite packs free commercial use" \
  opengameart.org itch.io

run office_sim_theme_assets \
  "pixel office desk terminal worker sprite pack free license" \
  opengameart.org itch.io

echo "done"
