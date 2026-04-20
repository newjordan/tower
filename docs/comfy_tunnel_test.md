# ComfyUI Tunnel Integration Test

Use this once `COMFY_API_URL` is available.

Important: if your server is running with default `ASSET_ROUTING_MODE=openai_only`, Comfy lane will be forced off. For this test run:

```bash
export ASSET_ROUTING_MODE="class_router"
```

## Required env

```bash
export COMFY_API_URL="https://your-comfy-tunnel"
export COMFY_WORKFLOW_FILE="/absolute/path/to/workflow.json"
```

Quick wiring test option:
- `COMFY_WORKFLOW_FILE=assets/workflows/comfy/workflow_stub.json`
- then set `COMFY_TEST_ACCEPT_FALLBACK=1` when running the smoke test.

Optional auth:

```bash
export COMFY_AUTH_HEADER="X-Comfy-Token"
export COMFY_AUTH_TOKEN="..."
# or
export COMFY_BASIC_USER="..."
export COMFY_BASIC_PASS="..."
```

## Run smoke test

```bash
scripts/test_comfy_tunnel.sh
```

Default probe spec: `assets/workflows/comfy/comfy_tunnel_probe.json`.

## Exit codes

- `0`: tunnel + backend comfy lane healthy
- `2`: tunnel unreachable
- `6`: comfy lane reached but generation fell back (`comfy_api_error_fallback`)
- `7`: backend missing comfy env on startup (`comfy_not_configured_placeholder`)

Set `COMFY_TEST_ACCEPT_FALLBACK=1` to treat fallback as acceptable during bring-up.
