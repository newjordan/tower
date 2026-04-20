#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${EXA_API_KEY:-}" ]]; then
  echo "EXA_API_KEY is required"
  exit 1
fi

OUT_DIR="${1:-research/exa}"
mkdir -p "$OUT_DIR"

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"

search() {
  local name="$1"
  local query="$2"
  local payload
  payload="$(jq -n --arg q "$query" '{query:$q, numResults:8, type:"auto"}')"
  curl -sS https://api.exa.ai/search \
    -H "content-type: application/json" \
    -H "x-api-key: ${EXA_API_KEY}" \
    --data "$payload" > "${OUT_DIR}/${timestamp}_${name}_search.json"
  echo "saved ${OUT_DIR}/${timestamp}_${name}_search.json"
}

research() {
  local name="$1"
  local instruction="$2"
  local payload
  payload="$(jq -n --arg i "$instruction" '{instructions:$i, model:"exa-research"}')"
  curl -sS https://api.exa.ai/research/v0/tasks \
    -H "content-type: application/json" \
    -H "x-api-key: ${EXA_API_KEY}" \
    --data "$payload" > "${OUT_DIR}/${timestamp}_${name}_research_task.json"
  echo "saved ${OUT_DIR}/${timestamp}_${name}_research_task.json"
}

search "pixel_models" "best API models for 8-bit pixel art sprites and animations 2026"
search "sprite_pipeline" "automation tools for sprite sheets and pixel animation export"
search "pricing_latency" "image generation API pricing latency comparison for game asset pipelines"
search "licenses" "commercial usage terms AI generated game assets APIs"

research "weekly_model_bakeoff" \
  "Find top current tools/models for 8-bit game asset generation and animation with API automation support. Compare quality consistency, controllability, batch support, pricing, and licensing. Return a ranked recommendation for a production pipeline."

echo "done"
