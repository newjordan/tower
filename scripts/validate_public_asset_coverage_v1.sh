#!/usr/bin/env bash
set -euo pipefail

SELECTED_PACKS_PATH="${SELECTED_PACKS_PATH:-assets/public/v1/selected_packs.json}"
BACKLOG_DIR="${BACKLOG_DIR:-assets/backlog/v1}"

python3 - "$SELECTED_PACKS_PATH" "$BACKLOG_DIR" <<'PY'
import json
import sys
from pathlib import Path

selected_path = Path(sys.argv[1])
backlog_dir = Path(sys.argv[2])

if not selected_path.exists():
    raise SystemExit(f"missing selected packs file: {selected_path}")
if not backlog_dir.exists():
    raise SystemExit(f"missing backlog dir: {backlog_dir}")

selected = json.loads(selected_path.read_text(encoding="utf-8"))
packs = selected.get("packs", [])

asset_ids = set()
manifest_errors = []
for pack in packs:
    out_dir = pack.get("import_output_dir")
    status = pack.get("status", "")
    if not out_dir:
        if status.startswith("imported"):
            manifest_errors.append(f"{pack.get('pack_id')}: imported status but no import_output_dir")
        continue
    manifest_path = Path(out_dir) / "manifest.json"
    if not manifest_path.exists():
        manifest_errors.append(f"{pack.get('pack_id')}: missing manifest {manifest_path}")
        continue
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except Exception as exc:
        manifest_errors.append(f"{pack.get('pack_id')}: failed to parse manifest ({exc})")
        continue
    for asset in manifest.get("assets", []):
        aid = asset.get("asset_id")
        if isinstance(aid, str) and aid:
            asset_ids.add(aid)

backlog_ids = sorted(p.stem for p in backlog_dir.glob("*.json"))
missing = [aid for aid in backlog_ids if aid not in asset_ids]

print(f"indexed_public_assets={len(asset_ids)}")
print(f"backlog_ids={len(backlog_ids)}")
print(f"missing_backlog_ids={len(missing)}")

if manifest_errors:
    print("manifest_errors:")
    for e in manifest_errors:
        print(f"  - {e}")

if missing:
    print("missing_ids:")
    for aid in missing:
        print(f"  - {aid}")
    raise SystemExit(1)

if manifest_errors:
    raise SystemExit(1)

print("coverage_ok=true")
PY
