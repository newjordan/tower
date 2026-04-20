#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-http://127.0.0.1:3000}"
target="${2:-assets/backlog/v1}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)"
out_dir="assets/backlog/runs/${run_id}"
mkdir -p "$out_dir"

collect_files() {
  local t="$1"
  if [[ -f "$t" ]]; then
    printf '%s\n' "$t"
    return
  fi
  if [[ -d "$t" ]]; then
    find "$t" -maxdepth 1 -type f -name '*.json' | sort
    return
  fi
  echo "target not found: $t" >&2
  return 1
}

mapfile -t files < <(collect_files "$target")
if [[ ${#files[@]} -eq 0 ]]; then
  echo "no spec files found under $target" >&2
  exit 1
fi

ok=0
failed=0
for spec in "${files[@]}"; do
  name="$(basename "$spec" .json)"
  resp_file="$out_dir/${name}.response.json"

  http_code="$(curl -sS -o "$resp_file" -w '%{http_code}' \
    -X POST "$base_url/assets/generate" \
    -H 'content-type: application/json' \
    --data @"$spec")"

  if [[ "$http_code" != "200" ]]; then
    echo "FAIL $name http=$http_code response=$resp_file"
    failed=$((failed + 1))
    continue
  fi

  if command -v jq >/dev/null 2>&1; then
    asset_id="$(jq -r '.manifest.asset_id // "unknown"' "$resp_file")"
    status="$(jq -r '.manifest.generator_status // "unknown"' "$resp_file")"
    ready="$(jq -r '.manifest.status // "unknown"' "$resp_file")"
    echo "OK   $name asset_id=$asset_id status=$status ready=$ready"
  else
    echo "OK   $name response=$resp_file"
  fi
  ok=$((ok + 1))
done

echo "run_id=$run_id ok=$ok failed=$failed output_dir=$out_dir"
if [[ $failed -gt 0 ]]; then
  exit 2
fi
