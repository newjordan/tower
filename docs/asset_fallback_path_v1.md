# Asset Fallback Path v1

## Trigger conditions
Fallback placeholder path is activated when:
- provider env is missing (`*_not_configured_placeholder`)
- provider returns success without an image URL (`*_api_success_no_image_url`)
- provider returns an error (`*_api_error_fallback`)

## Fallback flow
1. Write class-specific placeholder payload to `raw_output.txt`.
2. Normalize output with palette stamp via `scripts/aseprite_normalize.sh`.
3. Build deterministic `sprite_sheet.txt` placeholder sheet.
4. Emit metadata with `placeholder_used=true` and palette checksum.
5. Mark manifest status as `placeholder_ready`.

## Class coverage
Every required class has a placeholder template:
- `agent_sprite`
- `department_prop`
- `environment_tile`
- `ui_overlay`
- `fx`
- `portrait`
