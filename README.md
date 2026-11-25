# Agent Tower Sim

Observer-first tower simulation: 100 autonomous agents, a 1 Hz authoritative tick, and live WebSocket diffs.

The simulation rules are seeded. `initialize_tower(seed)` followed by the same number of `run_tick` calls produces the same decisions, economy, leases, and floor progression. `sim::state_fingerprint` is the regression digest for that state. Random UUIDs and wall-clock timestamps stay on events and ids, and they do not feed the next tick.

When two goal scores tie, the earlier candidate wins, in this order: eat, rest, socialize, relocate, work. When two floor types are overloaded equally often, the higher rank wins (utility, then service, commercial, residential) before the usual expansion map runs. A strictly higher score or overload count still wins on its own.

## Requirements

- Rust 1.100.0 (edition 2024). `rust-toolchain.toml` pins the compiler, rustfmt, and clippy.
- Optional Postgres and Redis for persistent mode. Without `DATABASE_URL` and `REDIS_URL`, the server stays in memory.

## Run the server

```bash
cargo run
```

Open `http://localhost:3000`.

Asset generation defaults to off:

- `ASSET_GENERATION_DISABLED=1` when the variable is unset.
- Set `ASSET_GENERATION_DISABLED=0` only to restore the legacy generator.

### Run summary

The example binary advances a tower and prints the economy, goals, floors, and fingerprint:

```bash
cargo run --example run_summary -- --seed 42 --ticks 200
```

`--seed` defaults to 42 and `--ticks` defaults to 200. Run the same command twice and the fingerprint line matches.

### Optional persistent mode

Start infra:

```bash
docker compose up -d
```

Run the server with persistence enabled:

```bash
DATABASE_URL=postgres://tower:tower@localhost:5432/tower \
REDIS_URL=redis://127.0.0.1:6379 \
cargo run
```

- Redis stores the latest hot snapshot at `tower:{tower_id}:snapshot`.
- Postgres stores:
  - `tower_snapshots` (durable snapshot every 30 ticks)
  - `tower_metrics` (every tick)
  - `tower_events` (every new event)
  - `ops_department_states` (latest state per floor/department)
  - `ops_desk_activity` (latest desk activity windows)
  - `ops_floor_usage` (usage counters for prestige/rankings)
- On startup, the server hydrates the latest snapshot, recent metrics/events, and ops state when they exist.

## Benchmarks

Tick throughput is measured with criterion on a 100-agent tower (`cargo bench --bench tick_throughput`). Numbers below are from one release run on 2026-11-25, Rust 1.100.0, Linux x86_64, 4 vCPU Intel Xeon. Criterion reports the median and the 95% confidence interval.

| Benchmark | What it times | Median | Throughput |
| --- | --- | --- | --- |
| `tower_tick/steady_tick` | One `run_tick` after a 100-tick warmup, seed `0xA11CE000` | 86.382 µs | 11,576 ticks/s (11.414–11.714k) |
| `tower_tick/hundred_ticks_after_warmup` | 100 ticks after a 50-tick warmup, same seed | 9.591 ms / 100 ticks | 10,426 ticks/s (10.315–10.501k) |

## Tests and CI

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo bench --no-run
```

`cargo test` covers the existing unit tests plus:

- Determinism: two runs from the same seed produce the same canonical state and the same fingerprint after N ticks. Pinned fingerprints lock seeds 1, 7, 42, and 99.
- Property tests (proptest): for random seeds and short horizons, agent count stays 100, needs and mood stay in range, credits stay non-negative, leases stay consistent, floor occupancy matches agent locations, furniture stays within the stage cap, and pipelines stay in range.

GitHub Actions (`.github/workflows/ci.yml`) runs fmt, clippy, tests, the example, and a benchmark compile on Rust 1.100.0.

## HTTP API

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
- `GET /` browser dashboard

The world loop is the 100-agent simulation above, with needs, economy, lease delinquency, eviction, tower health, and an event log. Runtime floor tilemaps use semantic wall ids (`tile_wall_corner_*`, `tile_wall_edge_*`, and the rest). Snapshots include runtime entity mappings (`agent` / `prop` / `ui` / `fx`) and per-floor placement overlays. Curated public packs are served from `/assets/public/v1/...`.

### Control examples

Set a policy:

```bash
curl -X POST http://localhost:3000/tower/tower-1/policy \
  -H "content-type: application/json" \
  -d '{"key":"wage_boost","value":0.25,"ttl_s":120}'
```

Broadcast a signal:

```bash
curl -X POST http://localhost:3000/tower/tower-1/signal \
  -H "content-type: application/json" \
  -d '{"signal_type":"crisis_food","target_zone":"all","intensity":0.8,"duration_s":60}'
```

WebSocket subscribe filter:

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

Ingest an OpenClaw event:

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

Department state, prestige, and rankings:

```bash
curl "http://localhost:3000/tower/tower-1/departments/coding/state?floor_id=floor-a"
curl "http://localhost:3000/tower/tower-1/prestige"
curl "http://localhost:3000/tower/tower-1/rankings"
```

Asset generation is disabled by default and returns `503`:

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

```json
{
  "status": "disabled",
  "reason": "asset_generation_pipeline_disabled_v1",
  "message": "POST /assets/generate is disabled while we migrate to curated public sprite packs.",
  "hint": "Set ASSET_GENERATION_DISABLED=0 to re-enable the legacy generator temporarily."
}
```

Existing manifests and the curated catalog:

```bash
curl "http://localhost:3000/assets/<asset_id>/manifest"
curl "http://localhost:3000/assets/public/v1/catalog"
curl "http://localhost:3000/assets/public/v1/tiles/tile_wall_corner_top_left" > /tmp/tile_wall_corner_top_left.png
curl "http://localhost:3000/assets/public/v1/tiles/agent_coder_idle" > /tmp/agent_coder_idle.png
```

Runtime mapping notes:

- Dashboard art resolution covers floor tilemap ids and runtime entity mappings via `/assets/public/v1/catalog`.
- `WorldSnapshot` includes `runtime_assets` with categories `agents`, `props`, `ui`, and `fx`.
- `WorldSnapshot` also includes `runtime_floor_placements` for per-floor `prop` / `agent` / `fx` coordinates.
- `WorldDiff` includes `runtime_assets_delta` (upserts and removals) and `runtime_floor_placements_delta` (per-floor upserts, removals, and `removed_floor_ids`).
- Floor agent overlays resolve sprite ids from `runtime_assets.agents` by `entity_id`.
- Agent visual changes, in-floor repositioning, and floor transitions are cadence-throttled. Optional non-urgent floor transitions are capped at 3 per tick and 2 per target floor.
- The tick loop also emits a full `WorldSnapshot` every 5 ticks.

OpenClaw event types:

- `task_started`
- `task_completed`
- `task_failed`
- `git_pull`
- `git_commit`
- `git_push`
- `test_run`

## Asset references

Historical generator notes (disabled as of February 24, 2026):

- Legacy mode: `docs/openai_asset_mode.md`
- Style bible and geometry: `docs/lowfi_style_bible_v1.md`
- Fallback behavior: `docs/asset_fallback_path_v1.md`
- Backlog payloads: `assets/backlog/v1/`

v1 references:

- Style bible: `docs/lowfi_style_bible_v1.md`
- Backlog pack: `assets/backlog/v1/`
- OpenAI mode: `docs/openai_asset_mode.md`
- Public replacement shortlist: `docs/public_asset_replacements_v1.md`
- Exa search helper: `scripts/exa_public_asset_search.sh`
- Selected public packs: `assets/public/v1/selected_packs.json`
- Public catalog module: `src/public_assets.rs`
- Import and validation scripts: `scripts/import_perspective_walls_v1.sh`, `scripts/alias_perspective_walls_semantic_v1.sh`, `scripts/import_urban_character_pack_v1.sh`, `scripts/import_lpc_office_v1.sh`, `scripts/validate_public_asset_coverage_v1.sh`
- World diff churn probe: `scripts/probe_world_diff_churn_v1.sh`
- Runtime floor preview: `scripts/render_runtime_floor_preview_v1.sh`

Legacy backlog execution, only after the generator is re-enabled:

```bash
scripts/run_asset_backlog_v1.sh
scripts/test_openai_subscription.sh
```
