#!/usr/bin/env bash
set -euo pipefail

TARGET_DIFFS="${1:-12}"
if ! [[ "$TARGET_DIFFS" =~ ^[0-9]+$ ]] || [ "$TARGET_DIFFS" -le 0 ]; then
  echo "usage: scripts/probe_world_diff_churn_v1.sh [diff_count>0]" >&2
  exit 2
fi

SERVER_LOG="${TOWER_WS_PROBE_SERVER_LOG:-/tmp/tower-ws-probe-server.log}"
BOOT_TIMEOUT_S="${TOWER_WS_PROBE_BOOT_TIMEOUT_S:-120}"
WS_TIMEOUT_MS="${TOWER_WS_PROBE_WS_TIMEOUT_MS:-30000}"

cargo run >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
cleanup() {
  kill "$SERVER_PID" >/dev/null 2>&1 || true
  wait "$SERVER_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 "$BOOT_TIMEOUT_S"); do
  if rg -q "server listening on 0.0.0.0:3000" "$SERVER_LOG"; then
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

node - "$TARGET_DIFFS" "$WS_TIMEOUT_MS" <<'NODE'
const target = Number(process.argv[2] || '12');
const timeoutMs = Number(process.argv[3] || '30000');
const url = 'ws://127.0.0.1:3000/ws/tower/tower-1';

const acc = {
  diffs: 0,
  assetUpsertAgents: 0,
  assetUpsertProps: 0,
  assetUpsertUi: 0,
  assetUpsertFx: 0,
  assetRemoveAgents: 0,
  assetRemoveProps: 0,
  assetRemoveUi: 0,
  assetRemoveFx: 0,
  floorUpsertAgents: 0,
  floorUpsertProps: 0,
  floorUpsertFx: 0,
  floorRemoveAgents: 0,
  floorRemoveProps: 0,
  floorRemoveFx: 0,
  floorRemovedFloors: 0,
};

const ws = new WebSocket(url);
let done = false;

function decodeData(data) {
  if (typeof data === 'string') return data;
  if (data instanceof ArrayBuffer) return Buffer.from(data).toString('utf8');
  if (ArrayBuffer.isView(data)) {
    return Buffer.from(data.buffer, data.byteOffset, data.byteLength).toString('utf8');
  }
  if (data && typeof data.arrayBuffer === 'function') {
    return data.arrayBuffer().then((ab) => Buffer.from(ab).toString('utf8'));
  }
  return '';
}

function finish(exitCode = 0) {
  if (done) return;
  done = true;
  const d = acc.diffs || 1;
  const out = {
    diffs: acc.diffs,
    avg_asset_upsert_agents: Number((acc.assetUpsertAgents / d).toFixed(2)),
    avg_asset_upsert_props: Number((acc.assetUpsertProps / d).toFixed(2)),
    avg_asset_upsert_ui: Number((acc.assetUpsertUi / d).toFixed(2)),
    avg_asset_upsert_fx: Number((acc.assetUpsertFx / d).toFixed(2)),
    avg_asset_remove_agents: Number((acc.assetRemoveAgents / d).toFixed(2)),
    avg_asset_remove_props: Number((acc.assetRemoveProps / d).toFixed(2)),
    avg_asset_remove_ui: Number((acc.assetRemoveUi / d).toFixed(2)),
    avg_asset_remove_fx: Number((acc.assetRemoveFx / d).toFixed(2)),
    avg_floor_upsert_agents: Number((acc.floorUpsertAgents / d).toFixed(2)),
    avg_floor_upsert_props: Number((acc.floorUpsertProps / d).toFixed(2)),
    avg_floor_upsert_fx: Number((acc.floorUpsertFx / d).toFixed(2)),
    avg_floor_remove_agents: Number((acc.floorRemoveAgents / d).toFixed(2)),
    avg_floor_remove_props: Number((acc.floorRemoveProps / d).toFixed(2)),
    avg_floor_remove_fx: Number((acc.floorRemoveFx / d).toFixed(2)),
    avg_floor_removed_floors: Number((acc.floorRemovedFloors / d).toFixed(2)),
  };
  console.log(JSON.stringify(out));
  try {
    ws.close();
  } catch (_) {}
  process.exit(exitCode);
}

const timer = setTimeout(() => finish(acc.diffs > 0 ? 0 : 2), timeoutMs);

ws.onmessage = async (event) => {
  try {
    const decoded = decodeData(event.data);
    const text = typeof decoded === 'string' ? decoded : await decoded;
    if (!text) return;
    const msg = JSON.parse(text);
    if (msg.type !== 'WorldDiff') return;
    const payload = msg.payload || {};
    const assets = payload.runtime_assets_delta || {};
    const floorDelta = payload.runtime_floor_placements_delta || {};
    const floors = Array.isArray(floorDelta.floors) ? floorDelta.floors : [];

    acc.diffs += 1;
    acc.assetUpsertAgents += (assets.upsert_agents || []).length;
    acc.assetUpsertProps += (assets.upsert_props || []).length;
    acc.assetUpsertUi += (assets.upsert_ui || []).length;
    acc.assetUpsertFx += (assets.upsert_fx || []).length;
    acc.assetRemoveAgents += (assets.remove_agent_ids || []).length;
    acc.assetRemoveProps += (assets.remove_prop_ids || []).length;
    acc.assetRemoveUi += (assets.remove_ui_ids || []).length;
    acc.assetRemoveFx += (assets.remove_fx_ids || []).length;

    acc.floorRemovedFloors += (floorDelta.removed_floor_ids || []).length;
    for (const floor of floors) {
      acc.floorUpsertAgents += (floor.upsert_agents || []).length;
      acc.floorUpsertProps += (floor.upsert_props || []).length;
      acc.floorUpsertFx += (floor.upsert_fx || []).length;
      acc.floorRemoveAgents += (floor.remove_agent_ids || []).length;
      acc.floorRemoveProps += (floor.remove_prop_ids || []).length;
      acc.floorRemoveFx += (floor.remove_fx_ids || []).length;
    }

    if (acc.diffs >= target) {
      clearTimeout(timer);
      finish(0);
    }
  } catch (err) {
    console.error(String(err));
  }
};

ws.onerror = () => {
  clearTimeout(timer);
  finish(2);
};

ws.onclose = () => {
  if (!done) {
    clearTimeout(timer);
    finish(acc.diffs > 0 ? 0 : 2);
  }
};
NODE
