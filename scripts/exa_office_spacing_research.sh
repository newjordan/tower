#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${EXA_API_KEY:-}" ]]; then
  echo "EXA_API_KEY is required"
  exit 1
fi

OUT_DIR="${1:-research/exa}"
mkdir -p "$OUT_DIR"
TS="$(date -u +%Y%m%dT%H%M%SZ)"

run() {
  local name="$1"
  local query="$2"
  local payload
  payload="$(jq -n --arg q "$query" '{query:$q, numResults:12, type:"auto"}')"
  curl -sS https://api.exa.ai/search \
    -H "content-type: application/json" \
    -H "x-api-key: ${EXA_API_KEY}" \
    --data "$payload" > "${OUT_DIR}/${TS}_${name}.json"
  echo "saved ${OUT_DIR}/${TS}_${name}.json"
}

run office_spacing_standards \
  "office layout spacing standards ADA OSHA egress widths workstation circulation US"

run office_occupancy_density \
  "office occupant load factor business area 150 gross sf per occupant building code"

run office_layout_best_practices \
  "office floorplan best practices aisle widths desk spacing collaborative zones"

run cubicle_density_planning \
  "open office cubicle density planning ratios seats to circulation area"

echo "done"
