# OpenAI Asset Mode (Current)

This project is now configured to use OpenAI as the active generation lane by default.

## Routing mode
- Default: `ASSET_ROUTING_MODE=openai_only`
- Optional legacy mode: `ASSET_ROUTING_MODE=class_router`

In `openai_only` mode:
- Every request routes to `open_ai_images`.
- Any non-OpenAI `preferred_lane` is ignored and recorded in metadata as forced routing.
- No Comfy/Replicate/Scenario provider call is made.

## Required env

```bash
export OPENAI_API_KEY="..."
```

Optional:

```bash
export OPENAI_IMAGE_MODEL="gpt-image-1"
export OPENAI_IMAGE_SIZE="1024x1024"
```

## Smoke test

```bash
scripts/test_openai_subscription.sh
```

Default probe spec:
- `assets/backlog/v1/openai_subscription_probe.json`
