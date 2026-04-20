# Backlog v1 (Executable)

This folder contains ready-to-POST request payloads for `POST /assets/generate`.

## Files to generate first

1. `agent_coder_idle.json`
2. `agent_coder_work.json`
3. `agent_coder_fail.json`
4. `prop_pull_desk.json`
5. `prop_commit_desk.json`
6. `prop_push_desk.json`
7. `prop_test_desk.json`
8. `prop_review_desk.json`
9. `prop_planning_board.json`
10. `tile_floor_clean.json`
11. `tile_wall_terminal.json`
12. `tile_window_night.json`
13. `ui_state_healthy.json`
14. `ui_state_degraded.json`
15. `ui_state_stalled.json`
16. `ui_state_broken.json`
17. `ui_prestige_star.json`
18. `fx_upload_burst.json`
19. `fx_fail_pop.json`
20. `fx_spark_flicker.json`
21. `portrait_coding_neutral.json`
22. `portrait_coding_thinking.json`
23. `portrait_coding_warning.json`
24. `portrait_coding_success.json`
25. `openai_subscription_probe.json`

## Run all specs

```bash
scripts/run_asset_backlog_v1.sh
```

## Run only one file

```bash
scripts/run_asset_backlog_v1.sh http://127.0.0.1:3000 assets/backlog/v1/openai_subscription_probe.json
```
