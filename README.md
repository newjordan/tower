# Agent Tower Sim

Observer-first tower simulation with autonomous agents, a 1s authoritative tick loop, and live WebSocket diffs.

## What is implemented

- Rust backend (`axum`) with:
- `POST /session/guest`
- `GET /tower/tower-1/bootstrap`
- `GET /tower/tower-1/metrics`
- `GET /tower/tower-1/events?cursor=&limit=`
- `GET /tower/tower-1/explain?agent_id=...`
- `GET /tower/tower-1/departments/coding/state?floor_id=floor-a`
- `GET /tower/tower-1/prestige`
- `GET /tower/tower-1/rankings`
- `POST /events/openclaw`
- `POST /assets/generate`
- `GET /assets/{asset_id}/manifest`
- `POST /tower/tower-1/policy`
- `POST /tower/tower-1/signal`
- `GET /ws/tower/tower-1` (live stream)
- 100-agent deterministic simulation loop at 1Hz
- Needs/economy/lease delinquency + eviction flow
- Tower health metrics and event log
- Browser dashboard at `/`
- Runtime floor tilemaps with semantic wall IDs (`tile_wall_corner_*`, `tile_wall_edge_*`, etc.)
- Runtime entity asset mappings for `agent`/`prop`/`ui`/`fx` IDs in `WorldSnapshot.runtime_assets`
- Runtime per-floor placement overlays for `prop`/`agent`/`fx` in `WorldSnapshot.runtime_floor_placements`
- Public asset catalog endpoint + tile image serving for curated packs (`/assets/public/v1/...`)

## Run

```bash
cargo run
```

Open `http://localhost:3000`.

Asset generation default:
- `ASSET_GENERATION_DISABLED=1` (default when unset).
- Set `ASSET_GENERATION_DISABLED=0` only if you intentionally want legacy generator behavior.

### Optional persistent mode (Postgres + Redis)

Start infra:

```bash
docker compose up -d
```

Run server with persistence enabled:

```bash
DATABASE_URL=postgres://tower:tower@localhost:5432/tower \
REDIS_URL=redis://127.0.0.1:6379 \
cargo run
```

Behavior:
- Redis stores latest hot snapshot at `tower:{tower_id}:snapshot`
- Postgres stores:
- `tower_snapshots` (durable snapshot every 30 ticks)
- `tower_metrics` (every tick)
- `tower_events` (every new event)
- `ops_department_states` (latest state per floor/department)
- `ops_desk_activity` (latest desk activity windows)
- `ops_floor_usage` (usage counters for prestige/rankings)
- On startup, server hydrates latest snapshot and recent metrics/events when available
- On startup, ops state (department/desk/usage) is also hydrated from Postgres

## Control API examples

Set policy:

```bash
curl -X POST http://localhost:3000/tower/tower-1/policy \
  -H "content-type: application/json" \
  -d '{"key":"wage_boost","value":0.25,"ttl_s":120}'
```

Broadcast signal:

```bash
curl -X POST http://localhost:3000/tower/tower-1/signal \
  -H "content-type: application/json" \
  -d '{"signal_type":"crisis_food","target_zone":"all","intensity":0.8,"duration_s":60}'
```

WebSocket subscribe filter example:

```json
{
  "type": "view.subscribe",
  "payload": {
    "channels": ["alerts", "metrics"],
    "agent_filter": null,
    "zone_filter": null
  }
}
```

Ingest OpenClaw event:

```bash
curl -X POST http://localhost:3000/events/openclaw \
  -H "content-type: application/json" \
  -d '{
    "event_id":"11111111-1111-1111-1111-111111111111",
    "timestamp":"2026-02-24T16:00:00Z",
    "tower_id":"tower-1",
    "floor_id":"floor-a",
    "department_id":"coding",
    "event_type":"task_completed",
    "task_id":"task-42",
    "agent_id":"coder-1",
    "metadata":{"repo":"tower"}
  }'
```

Get coding department state for a floor:

```bash
curl "http://localhost:3000/tower/tower-1/departments/coding/state?floor_id=floor-a"
```

Get usage-based prestige/rankings:

```bash
curl "http://localhost:3000/tower/tower-1/prestige"
curl "http://localhost:3000/tower/tower-1/rankings"
```

Asset generation endpoint (disabled by default):

```bash
curl -X POST http://localhost:3000/assets/generate \
  -H "content-type: application/json" \
  -d '{
    "spec": {
      "tower_id":"tower-1",
      "floor_id":"floor-a",
      "department_id":"coding",
      "name":"push_desk_idle",
      "asset_class":"department_prop",
      "width":32,
      "height":32,
      "frames":6,
      "tags":["coding","desk","idle"],
      "palette_id":"tower_v1",
      "prompt":"8-bit pixel art push desk with terminal lights blinking"
    }
  }'
```

Current response is `503 Service Unavailable`:

```json
{
  "status": "disabled",
  "reason": "asset_generation_pipeline_disabled_v1",
  "message": "POST /assets/generate is disabled while we migrate to curated public sprite packs.",
  "hint": "Set ASSET_GENERATION_DISABLED=0 to re-enable the legacy generator temporarily."
}
```

Get existing generated manifest (for already-created assets):

```bash
curl "http://localhost:3000/assets/<asset_id>/manifest"
```

Get curated public asset catalog:

```bash
curl "http://localhost:3000/assets/public/v1/catalog"
```

Get a curated public asset image by asset ID:

```bash
curl "http://localhost:3000/assets/public/v1/tiles/tile_wall_corner_top_left" > /tmp/tile_wall_corner_top_left.png
curl "http://localhost:3000/assets/public/v1/tiles/agent_coder_idle" > /tmp/agent_coder_idle.png
```

Runtime mapping notes:
- Dashboard art resolution now covers floor tilemap IDs and runtime entity mappings via `/assets/public/v1/catalog`.
- `WorldSnapshot` includes `runtime_assets` with categories `agents`, `props`, `ui`, and `fx`.
- `WorldSnapshot` also includes `runtime_floor_placements` for per-floor `prop`/`agent`/`fx` placement coordinates.
- `WorldDiff` includes `runtime_assets_delta` as runtime-entity upsert/removal patches (`upsert_*`, `remove_*`) keyed to visual asset changes.
- `WorldDiff` also includes `runtime_floor_placements_delta` as per-floor entity upsert/removal patches (`upsert_*`, `remove_*`, `removed_floor_ids`) keyed to visual placement changes (agents: `x`/`y`; props/fx: `asset_id`/`x`/`y`).
- Dashboard floor agent overlays resolve sprite IDs from `runtime_assets.agents` by `entity_id` so agent visual changes stream through `runtime_assets_delta`.
- Agent visual mapping uses a short hysteresis cadence to reduce patch churn while preserving state transitions.
- Agent in-floor repositioning is cadence-throttled to reduce placement reshuffles.
- Agent floor transitions are cadence-throttled (20 ticks, except relocate/severe states) to reduce cross-floor patch churn.
- Optional non-urgent floor transitions are capped globally per tick (3) and per target floor (2) to avoid bursty cross-floor reshuffles.
- Tick loop also emits a full `WorldSnapshot` every 5 ticks for full-state refresh.

Historical generator notes (disabled as of February 24, 2026):
- Legacy mode details: `docs/openai_asset_mode.md`
- Style bible + geometry constraints: `docs/lowfi_style_bible_v1.md`
- Fallback behavior (legacy): `docs/asset_fallback_path_v1.md`
- Backlog request payloads (legacy): `assets/backlog/v1/`

v1 references:
- Style bible: `docs/lowfi_style_bible_v1.md`
- Backlog pack: `assets/backlog/v1/`
- OpenAI mode: `docs/openai_asset_mode.md`
- Public replacement shortlist: `docs/public_asset_replacements_v1.md`
- Exa search helper: `scripts/exa_public_asset_search.sh`
- Current selected public packs: `assets/public/v1/selected_packs.json`
- Public catalog module: `src/public_assets.rs`
- Wall import script (Perspective Walls Template): `scripts/import_perspective_walls_v1.sh`
- Wall semantic alias script: `scripts/alias_perspective_walls_semantic_v1.sh`
- Urban import script: `scripts/import_urban_character_pack_v1.sh`
- LPC office import script: `scripts/import_lpc_office_v1.sh`
- Public asset coverage validator: `scripts/validate_public_asset_coverage_v1.sh`
- World diff churn probe: `scripts/probe_world_diff_churn_v1.sh`
- Runtime floor preview renderer: `scripts/render_runtime_floor_preview_v1.sh`

Legacy backlog execution (only after generator is re-enabled):

```bash
scripts/run_asset_backlog_v1.sh
```

Legacy OpenAI subscription smoke test:

```bash
scripts/test_openai_subscription.sh
```

OpenClaw event types accepted in v0.2:
- `task_started`
- `task_completed`
- `task_failed`
- `git_pull`
- `git_commit`
- `git_push`
- `test_run`

## Tests

```bash
cargo test
```

Current unit coverage includes:
- need decay behavior
- deterministic goal selection
- rent debt to eviction flow
- health scoring penalty behavior
- policy/signal modifier effects

## Infra notes

`docker-compose.yml` provides local Postgres/Redis. If `DATABASE_URL` and `REDIS_URL` are unset, server runs fully in-memory.
