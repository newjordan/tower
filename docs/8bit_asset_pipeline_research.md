# 8-bit Asset + Animation Pipeline (Exa-Driven Research)

## Goal

Automate production of the art you need for Agent Tower floors (especially Coding Department) with a repeatable pipeline:

- concept -> pixel asset -> animation -> sprite sheet -> in-game manifest
- minimal manual bottlenecks
- easy model/tool swaps as better options appear

## Asset Needs Matrix

1. Characters / agents
- Idle, walking, working-at-desk, alert/failure, celebrate
- Directional variants (at least left/right; optional 4-dir)

2. Department props
- Pull desk, commit desk, push desk, test desk, review desk, planning board
- Small props: monitors, terminals, lamps, cable clutter, server rack, coffee machine

3. Environment tiles
- Floor/wall variants, windows, doors, wiring, signage, broken-state overlays

4. UI + status overlays
- Health states: healthy, degraded, stalled, broken
- Alerts, badges, progress bars, prestige/star marks

5. FX
- Flicker, sparks, upload burst, task-complete pulse, fail pop

6. Portraits / corner-office character frames
- Talk states (neutral, thinking, active, warning, success)

7. Packaging outputs
- Sprite sheets + metadata
- Stable naming convention
- Per-asset provenance (tool/model/prompt/date/version)

## Recommended Tooling Stack (Now)

1. Discovery + research intelligence
- Exa `search` and `research` to continuously evaluate models/tools/docs.

2. AI generation (images/sequences)
- Scenario API for production generation and batch processing (`txt2img`, `img2img`, custom generation tools, jobs, uploads).
- OpenAI Images for fast concept ideation and iterative edits.
- ComfyUI (local/cloud) for highly controlled workflows and custom pipeline graphs.
- Replicate for model hosting/runs and model search at runtime.

3. Pixel-art normalization + animation assembly
- Aseprite CLI + Lua scripting for palette control, frame cleanup, tags, sprite sheet export.

4. Modular content acceleration
- Asset Forge (+ Lua scripting) for quickly producing modular department props/silhouettes that can be pixel-rendered and adapted.

## Why This Stack

1. It separates responsibilities cleanly:
- Exa discovers
- Scenario/OpenAI/Comfy/Replicate generate
- Aseprite enforces game-ready pixel constraints

2. It keeps you future-proof:
- New model appears -> Exa finds it -> plug into generation stage without rewriting downstream tooling.

3. It matches your product rule:
- Art reflects real systems state; you need fast variants and consistent packaging.

## Automation Flow (v1)

1. Exa scouting job (daily)
- Query for best current pixel-art generation models/tools/workflows.
- Save ranked candidates + notes to `research/` JSON.

2. Asset request intake
- Input spec (e.g., `desk_push`, `state_broken`, `size=32x32`, `palette=tower_v1`).

3. Generation stage
- Route by asset class:
- Character/portrait: Scenario or Comfy workflow
- UI icons/fx: OpenAI image edits + Scenario variation batches
- Props/modular: Asset Forge scripted exports + pixel pass

4. Pixel normalization stage
- Aseprite CLI script:
- quantize to approved palette
- enforce transparent background rules
- enforce canvas sizes and anchor points

5. Animation stage
- Aseprite tags and frame timing templates:
- `idle`, `work`, `fail`, `recover`, `celebrate`

6. Packaging stage
- Export sprite sheet + JSON metadata
- Emit manifest with provenance:
- `generator`, `model_id`, `prompt_hash`, `palette_id`, `export_version`

7. Validation stage
- Reject assets if:
- wrong dimensions
- palette overflow
- missing tags
- alpha/background violations

8. Publish stage
- Store in your asset bucket + index in DB for runtime use by floor renderer.

## Exa Query Pack (Use With Your Account)

Use these query classes in rotation:

1. Model capability
- "best API model for pixel art sprite sheets with consistent character identity 2026"
- "AI tools for multi-frame sprite animation generation API"

2. Production reliability
- "API rate limits batch generation image models"
- "pricing and latency comparison for image generation APIs for games"

3. Pipeline integrations
- "Aseprite CLI automation examples sprite sheet JSON export"
- "ComfyUI workflow for pixel-art style transfer animation"

4. Legal/usage
- "commercial usage terms generated images API"
- "model license for generated game assets redistribution"

## Practical Decisions To Lock Next

1. Pixel spec baseline
- Base tile size, palette size, animation FPS per class, max frame counts.

2. Style bible
- 10-20 canonical references for tower, desks, agents, and failure states.

3. Throughput target
- e.g., "50 icon/fx assets + 10 animated character sets per week."

4. Cost envelope
- monthly generation budget and per-asset cost target.

## Initial Recommendation

Start with:

1. Scenario as primary batch generator
2. Aseprite CLI as hard gatekeeper for game-ready outputs
3. Exa daily scouting + weekly model bakeoff
4. ComfyUI/Replicate as overflow and experimental lanes

This is the fastest route to reliable, automatable 8-bit asset production while preserving quality control.

## Exa Run Findings (2026-02-24)

Official-domain Exa queries confirmed the following as highest-confidence building blocks:

1. Pixel normalization + animation packaging (hard gate)
- Aseprite CLI + scripting/API:
- https://www.aseprite.org/cli/
- https://www.aseprite.org/docs/scripting/
- https://www.aseprite.org/api/

2. Primary generation lane (batch game-asset oriented)
- Scenario API:
- https://docs.scenario.com/reference/posttxt2imginferences
- https://docs.scenario.com/reference/postimg2imginferences
- https://docs.scenario.com/docs/uploading-images
- https://docs.scenario.com/docs/third-party-model-generation

3. Secondary generation lane (fast ideation + edits)
- OpenAI Images API:
- https://platform.openai.com/docs/api-reference/images/createEdit
- https://platform.openai.com/docs/api-reference/images-streaming/image_edit
- https://platform.openai.com/docs/api-reference/batch/object

4. Model router / fallback execution lane
- Replicate API:
- https://replicate.com/docs/reference/http
- https://replicate.com/docs/topics/predictions/create-a-prediction
- https://replicate.com/changelog/2024-07-24-api-for-searching-public-models

5. Custom workflow lane for advanced control
- ComfyUI:
- https://docs.comfy.org/
- https://docs.comfy.org/specs/workflow_json
- https://docs.comfy.org/api-reference/cloud/workflow/submit-a-workflow-for-execution

### Ranked Production Stack

1. `Aseprite CLI + scripting` (non-negotiable output gate)
2. `Scenario API` (primary batch generation)
3. `OpenAI Images` (rapid concept + edit lane)
4. `Replicate` (model diversity + fallback)
5. `ComfyUI` (deep control/experimental pipelines)

### Gaps to Solve in Implementation

1. Stable palette enforcement profile (`tower_v1`) and validation script.
2. Animation template library (`idle/work/fail/recover/celebrate`) per agent archetype.
3. Provenance schema so each exported asset keeps source/model/prompt/version lineage.
4. Cost-aware router that chooses generation lane by asset class and budget.
