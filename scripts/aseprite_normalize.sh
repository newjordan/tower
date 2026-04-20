#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <input> <output> [palette_id]"
  exit 1
fi

in="$1"
out="$2"
palette_id="${3:-tower_v1}"
palette_file="assets/palettes/${palette_id}.hex"

if [[ ! -f "$in" ]]; then
  echo "input missing: $in"
  exit 1
fi

if [[ ! -f "$palette_file" ]]; then
  echo "unsupported palette_id: $palette_id (missing $palette_file)"
  exit 2
fi

palette_rows="$(grep -E '^#?[0-9A-Fa-f]{6}$' "$palette_file" | sed 's/^#//' | tr '[:upper:]' '[:lower:]')"
if [[ -z "$palette_rows" ]]; then
  echo "palette file contains no RGB rows: $palette_file"
  exit 3
fi
palette_checksum="$(printf "%s\n" "$palette_rows" | md5sum | awk '{print $1}')"

# Stubbed normalization path. If/when we pass image binaries into this script, run an
# aseprite indexed-color quantization pass here using the same palette checksum gate.
if command -v aseprite >/dev/null 2>&1; then
  :
fi

{
  echo "NORMALIZED_V1"
  echo "palette_id=$palette_id"
  echo "palette_checksum=$palette_checksum"
  echo "source_file=$(basename "$in")"
  echo "source:"
  cat "$in"
} >"$out"

echo "normalized $in -> $out with $palette_id ($palette_checksum)"
