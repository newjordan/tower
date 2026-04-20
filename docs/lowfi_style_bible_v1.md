# Agent Tower Art Style Bible v1 (Low-Fi Readability)

## Scope
This is a strict v1 rule set for observer-first readability. Fidelity can evolve later; legibility and deterministic production are non-negotiable now.

## Core Visual Thesis
Use a `traceable office flow` metaphor for software workflows:
- Traceable node = desk or station.
- Edge between nodes = corridor lane.
- Data packet/job = document token.
- Processing state = worker/NPC movement and desk activity.

This applies only when a software concept has a traceable node chain.

## Efficiency Guardrails (Required)
- Fixed-grid lane graph per room (no freeform nav).
- Deterministic movement routes (no per-tick pathfinding).
- Hard cap active courier NPCs per floor: `<= 12`.
- Reuse animation loops by asset class and state tags.

## Palette (Required)
- Palette id: `tower_v1`.
- Max colors: `16`.
- Transparent index: `0`.
- Canonical source files:
  - `assets/palettes/tower_v1.hex`
  - `assets/palettes/tower_v1.gpl`
- Any unknown `palette_id` is rejected at request validation.

## Asset-Class Geometry + Frame Constraints (Enforced)
- `agent_sprite`: `32x32`, frames `4..8`
- `department_prop`: `32x32`, frames `1..6`
- `environment_tile`: `16x16` or `32x32`, frames `1..4`
- `ui_overlay`: `16x16` or `32x32`, frames `1..4`
- `fx`: `16x16` or `32x32`, frames `2..8`
- `portrait`: `48x48`, frames `1..5`

## Silhouette + Readability Rules
- 1px outer silhouette priority over interior detail.
- Max 3 value bands per sprite region (shadow/mid/highlight).
- Keep contrast strongest at gameplay-relevant pixels (hands/tools/alerts).
- Background and foreground cannot share top-2 luminance bands.

## Naming + Prompt Rules
- Asset `name` must match `[a-z0-9_]+`.
- At least one tag is required.
- Prompts should include: role + activity + state + camera framing.

## Fallback Path (Required)
When providers fail, are unconfigured, or return no image URL:
- Generation writes class-specific placeholder payloads.
- Placeholder output is deterministic and includes fallback reason/status.
- Manifest status becomes `placeholder_ready`.
- Placeholder templates exist for all required classes:
  - `agent_sprite`, `department_prop`, `environment_tile`, `ui_overlay`, `fx`, `portrait`

## Manifest/Metadata Requirements
Each generated asset must carry:
- `style_version` (`lowfi_v1`)
- `palette_id`, `palette_checksum`, color set, transparent index
- `prompt_hash`
- placeholder fields (`placeholder_used`, `placeholder_key`)

## Production Principle
Any change that improves visual fidelity but reduces event-state readability is out of scope for v1.
