#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${EXA_API_KEY:-}" ]]; then
  echo "EXA_API_KEY is required"
  exit 1
fi

OUT_DIR="${1:-research/exa}"
mkdir -p "$OUT_DIR"

run_search() {
  local name="$1"
  local query="$2"
  shift 2
  local domains_json
  domains_json="$(printf '%s\n' "$@" | jq -R . | jq -s .)"
  local payload
  payload="$(jq -n --arg q "$query" --argjson d "$domains_json" '{query:$q, numResults:10, includeDomains:$d}')"
  curl -sS https://api.exa.ai/search \
    -H "content-type: application/json" \
    -H "x-api-key: ${EXA_API_KEY}" \
    --data "$payload" > "${OUT_DIR}/${name}.json"
  echo "saved ${OUT_DIR}/${name}.json"
}

run_search "official_scenario_search" \
  "Scenario API image generation jobs upload docs" \
  "docs.scenario.com"

run_search "official_openai_search" \
  "OpenAI image generation API docs edits variations" \
  "platform.openai.com"

run_search "official_replicate_search" \
  "Replicate API predictions model search docs" \
  "replicate.com"

run_search "official_aseprite_search" \
  "Aseprite command line scripting API docs" \
  "aseprite.org"

run_search "official_comfy_search" \
  "ComfyUI docs workflow API" \
  "docs.comfy.org" \
  "github.com"

echo "done"
