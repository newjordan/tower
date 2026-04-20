#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-http://127.0.0.1:3000}"
probe_spec="${2:-assets/workflows/comfy/comfy_tunnel_probe.json}"
accept_fallback="${COMFY_TEST_ACCEPT_FALLBACK:-0}"

: "${COMFY_API_URL:?set COMFY_API_URL}"
: "${COMFY_WORKFLOW_FILE:?set COMFY_WORKFLOW_FILE}"

if [[ ! -f "$probe_spec" ]]; then
  echo "probe spec missing: $probe_spec" >&2
  exit 1
fi

comfy_url="${COMFY_API_URL%/}/prompt"
probe_body="$(mktemp /tmp/comfy_tunnel_probe.XXXXXX.txt)"

curl_args=(curl -sS -o "$probe_body" -w '%{http_code}')
if [[ -n "${COMFY_AUTH_HEADER:-}" && -n "${COMFY_AUTH_TOKEN:-}" ]]; then
  curl_args+=(-H "${COMFY_AUTH_HEADER}: ${COMFY_AUTH_TOKEN}")
fi
if [[ -n "${COMFY_BASIC_USER:-}" ]]; then
  curl_args+=(-u "${COMFY_BASIC_USER}:${COMFY_BASIC_PASS:-}")
fi
curl_args+=("$comfy_url")

set +e
probe_code="$("${curl_args[@]}")"
probe_rc=$?
set -e
if [[ $probe_rc -ne 0 || "$probe_code" == "000" ]]; then
  echo "Comfy tunnel unreachable: $COMFY_API_URL" >&2
  exit 2
fi

echo "Comfy tunnel reachable: $COMFY_API_URL (GET /prompt http=$probe_code)"

gen_resp="$(mktemp /tmp/comfy_tunnel_generate.XXXXXX.json)"
http_code="$(curl -sS -o "$gen_resp" -w '%{http_code}' \
  -X POST "$base_url/assets/generate" \
  -H 'content-type: application/json' \
  --data @"$probe_spec")"

if [[ "$http_code" != "200" ]]; then
  echo "asset generate failed http=$http_code response=$gen_resp" >&2
  exit 3
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq required for assertions" >&2
  exit 4
fi

lane="$(jq -r '.manifest.generation_lane // ""' "$gen_resp")"
status="$(jq -r '.manifest.generator_status // ""' "$gen_resp")"
asset_id="$(jq -r '.manifest.asset_id // ""' "$gen_resp")"
ready="$(jq -r '.manifest.status // ""' "$gen_resp")"

echo "comfy probe: asset_id=$asset_id lane=$lane generator_status=$status status=$ready"

if [[ "$lane" != "comfy_ui" ]]; then
  echo "expected comfy_ui lane, got $lane" >&2
  exit 5
fi

case "$status" in
  comfy_api_success|comfy_api_success_no_image_url)
    exit 0
    ;;
  comfy_api_error_fallback)
    if [[ "$accept_fallback" == "1" ]]; then
      echo "fallback accepted via COMFY_TEST_ACCEPT_FALLBACK=1"
      exit 0
    fi
    echo "comfy call reached backend but generation fell back; check workflow/auth/tunnel" >&2
    exit 6
    ;;
  comfy_not_configured_placeholder)
    echo "backend process was started without COMFY_API_URL/COMFY_WORKFLOW_FILE" >&2
    echo "restart server with COMFY_* env vars exported" >&2
    exit 7
    ;;
  *)
    echo "unexpected generator_status: $status" >&2
    exit 8
    ;;
esac
