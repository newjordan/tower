# Public Asset Replacements v1 (Exa Search)

As of **February 24, 2026**, `/assets/generate` is intentionally disabled and replaced by a curated public-asset approach for v1 test environments.

## Selected now (v1 seed)

1. [LPC Revised] The Office
- https://opengameart.org/content/lpc-revised-the-office
- Fit: strong office furniture/props coverage for `department_prop` + environment detail.
- License: `CC-BY-SA 3.0` and `OGA-BY 3.0` (attribution required; share-alike implications to track).
- Import command: `scripts/import_lpc_office_v1.sh`
- Output: `assets/public/v1/imported/lpc_revised_the_office/tiles/prop_lpc_*.png`
- Compatibility aliases emitted: backlog prop/ui/fx/portrait IDs (including `prop_push_desk`, `ui_state_healthy`, `openai_subscription_probe`).
- Attribution file: `assets/public/v1/imported/lpc_revised_the_office/ATTRIBUTION.txt`
- Current import result: `292` assets (`271` base tiles + `21` compatibility aliases)

2. Urban Character Pack
- https://opengameart.org/content/urban-character-pack
- Fit: immediate `agent_sprite` baseline with multiple animations and chunky 8-bit look.
- License: `CC0` (lowest friction for prototype use).
- Import command: `scripts/import_urban_character_pack_v1.sh`
- Output: `assets/public/v1/imported/urban_character_pack/tiles/agent_urban_r*c*.png`
- Compatibility aliases emitted: `agent_coder_idle`, `agent_coder_work`, `agent_coder_fail` (+ worker variants).
- Current import result: `1281` assets (`1275` base cells + `6` compatibility aliases)

3. Perspective Walls Template
- https://opengameart.org/content/perspective-walls-template
- Fit: dedicated wall geometry/template source for `environment_tile` wall variants.
- License: `CC0` (lowest friction for prototype use).
- Import command: `scripts/import_perspective_walls_v1.sh`
- Semantic alias command: `scripts/alias_perspective_walls_semantic_v1.sh`
- Output: `assets/public/v1/imported/perspective_walls_template/tiles/tile_wall_r*c*.png`
- Compatibility aliases emitted: `tile_wall_terminal`, `tile_wall_primary`
- Semantic aliases emitted: `tile_wall_edge_*`, `tile_wall_corner_*`, `tile_wall_inner_corner_*`, `tile_wall_t_*`, `tile_wall_cross`, `tile_wall_door_*`, `tile_wall_window_*`, `tile_wall_pillar*`
- Semantic preview board: `assets/public/v1/imported/perspective_walls_template/semantic_alias_preview.png`
- Semantic review table: `assets/public/v1/imported/perspective_walls_template/semantic_alias_review.md`
- Semantic confidence summary: `13 high`, `11 medium`, `0 low`
- Current import result: `73` assets total (`48` base wall grid cells + `25` aliases, including semantic walls)

## Current v1 coverage

- Imported packs now loaded by runtime catalog: `3`
- Total indexed public PNG assets: `1646`
- Backlog compatibility coverage (`assets/backlog/v1/*.json`): `25/25`
- Coverage validator: `scripts/validate_public_asset_coverage_v1.sh`
- Runtime/entity mapping now consumes catalog IDs for `agent`, `prop`, `ui`, and `fx` groups (`WorldSnapshot.runtime_assets`).
- Runtime floor placement now consumes per-floor coordinates for `prop`/`agent`/`fx` overlays (`WorldSnapshot.runtime_floor_placements`).
- WebSocket flow now publishes `WorldDiff.runtime_assets_delta` as runtime-entity upsert/removal patches (`upsert_*`, `remove_*`) keyed to visual asset changes, and `WorldDiff.runtime_floor_placements_delta` as per-floor entity upsert/removal patches (`upsert_*`, `remove_*`, `removed_floor_ids`) keyed to visual placement changes (agents: `x`/`y`; props/fx: `asset_id`/`x`/`y`), plus a full `WorldSnapshot` every 5 ticks for periodic full refresh.
- Dashboard floor agent overlays resolve sprite IDs from `runtime_assets.agents` by `entity_id`, so agent visual swaps come from `runtime_assets_delta` rather than floor placement churn.
- Runtime agent mapping applies short hysteresis cadence to reduce unnecessary visual toggles and WS patch churn.
- Runtime in-floor repositioning now uses cadence throttling to reduce placement reshuffles.
- Runtime floor transitions now use a 20-tick cadence throttle (except relocate/severe states) to reduce cross-floor placement patch churn.
- Runtime optional non-urgent floor transitions are capped globally per tick (3) and per target floor (2) to reduce bursty cross-floor churn.
- World diff churn probe script: `scripts/probe_world_diff_churn_v1.sh`
- Runtime floor preview renderer: `scripts/render_runtime_floor_preview_v1.sh`

## Research Inputs

The pack selection above was driven by local Exa research runs, but the raw
search snapshots are intentionally excluded from git so the public repository
does not carry scratch research payloads, request metadata, or provider-specific
artifacts.

To reproduce or refresh that research locally, use:

- `scripts/exa_public_asset_search.sh`
- `scripts/exa_8bit_research.sh`
- `scripts/exa_official_queries.sh`

## Best immediate options

These are the strongest matches for low-color, blocky 8-bit/Atari-like prototype visuals.

1. Kenney 1-Bit Pack
- https://kenney-assets.itch.io/1-bit-pack
- Best for: high-contrast, low-color props/tiles/ui placeholders.

2. OpenGameArt Nat's 8x8 Starter Pack
- https://opengameart.org/content/nats-8x8-starter-pack
- Best for: tiny sprites and retro block silhouettes.

3. OpenGameArt Office Worker Sprites
- https://opengameart.org/content/office-worker-sprites
- Best for: `agent_sprite` replacements in office-themed floors.

4. OpenGameArt 2D Pixel Office Supplies 32x32
- https://opengameart.org/content/2d-pixel-office-supplies-32x32
- Best for: `department_prop` desk/office object replacements.

5. OpenGameArt Office 8x8 Tileset
- https://opengameart.org/content/office-8x8-tileset
- Best for: `environment_tile` variants with strict retro density.

6. OpenGameArt 1-Bit Platformer Pack
- https://opengameart.org/content/1-bit-platformer-pack
- Best for: fallback filler tiles/sprites when coverage gaps exist.

## Source pools for wider coverage

- itch.io CC0 8-bit assets: https://itch.io/game-assets/assets-cc0/tag-8-bit
- itch.io CC0 sprite assets: https://itch.io/game-assets/assets-cc0/tag-sprites
- OpenGameArt CC0 + OGA-BY collection: https://opengameart.org/content/cc0oga-by-pixel-art
- OpenGameArt CC0 resources: https://opengameart.org/content/cc0-resources
- LPC UI pack: https://lpc.opengameart.org/content/ui-pack
- Quintino CC0 assorted icons: https://quintino-pixels.itch.io/assorted-icons

## License checks before import

Use these references as verification points before each asset is committed:

- OpenGameArt OGA-BY 4.0 FAQ: https://opengameart.org/content/oga-by-40-faq
- OpenGameArt OGA-BY 3.0 FAQ: https://opengameart.org/content/oga-by-30-faq
- OpenGameArt general FAQ: https://opengameart.org/content/faq
- Kenney support/license entry point: https://kenney.nl/support
- Kenney terms: https://kenney.nl/terms-of-service

## Recommended v1 import policy

- Prefer CC0-only packs for test environment speed and minimal attribution friction.
- If using OGA-BY/CC-BY assets, store attribution text adjacent to each imported pack.
- Normalize all imported sprites to `tower_v1` via `scripts/aseprite_normalize.sh`.
- Keep one manifest per imported pack with source URL, license, and retrieval date.
