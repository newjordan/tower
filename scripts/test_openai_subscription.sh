#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-http://127.0.0.1:3000}"
probe_spec="${2:-assets/backlog/v1/openai_subscription_probe.json}"

: "${OPENAI_API_KEY:?set OPENAI_API_KEY}"

if [[ ! -f "$probe_spec" ]]; then
  echo "probe spec missing: $probe_spec" >&2
  exit 1
fi

resp_file="$(mktemp /tmp/openai_asset_probe.XXXXXX.json)"
http_code="$(curl -sS -o "$resp_file" -w '%{http_code}' \
  -X POST "$base_url/assets/generate" \
  -H 'content-type: application/json' \
  --data @"$probe_spec")"

if [[ "$http_code" != "200" ]]; then
  echo "asset generate failed http=$http_code response=$resp_file" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq required for assertions" >&2
  exit 3
fi

asset_id="$(jq -r '.manifest.asset_id // ""' "$resp_file")"
lane="$(jq -r '.manifest.generation_lane // ""' "$resp_file")"
status="$(jq -r '.manifest.generator_status // ""' "$resp_file")"
ready="$(jq -r '.manifest.status // ""' "$resp_file")"

echo "openai probe: asset_id=$asset_id lane=$lane generator_status=$status status=$ready"

if [[ "$lane" != "open_ai_images" ]]; then
  echo "expected open_ai_images lane, got $lane" >&2
  exit 4
fi

case "$status" in
  openai_api_success|openai_api_success_no_image_url)
    exit 0
    ;;
  openai_api_error_fallback)
    echo "OpenAI request reached backend but provider failed; verify API key/quota/model" >&2
    exit 5
    ;;
  openai_not_configured_placeholder)
    echo "backend process missing OPENAI_API_KEY in its runtime env" >&2
    exit 6
    ;;
  *)
    echo "unexpected generator_status: $status" >&2
    exit 7
    ;;
esac
