mod assets;
mod ops;
mod public_assets;
mod sim;
mod storage;

use assets::{AssetService, GenerateAssetRequest};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use ops::{
    CODING_DEPARTMENT, OpenClawEvent, OpsState, compute_floor_prestige, compute_rankings,
    get_department_state, process_event,
};
use serde::{Deserialize, Serialize};
use sim::{
    AgentGoal, DEFAULT_TOWER_ID, FloorType, MetricsPoint, RuntimeAssetBinding,
    RuntimeAssetMappingDeltas, RuntimeAssetMappings, RuntimeFloorPlacement,
    RuntimeFloorPlacementDelta, RuntimeFloorPlacementDeltas, RuntimeFloorPlacements,
    RuntimePlacedAsset, SimEvent, StreamPayload, TickResult, TowerState, apply_policy,
    broadcast_signal, ensure_floor_tilemaps, initialize_tower, run_tick,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use storage::Storage;
use tokio::sync::{Mutex, RwLock, broadcast};
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    world: Arc<Mutex<TowerState>>,
    metrics: Arc<RwLock<Vec<MetricsPoint>>>,
    events: Arc<RwLock<Vec<SimEvent>>>,
    decisions: Arc<RwLock<HashMap<Uuid, sim::DecisionTrace>>>,
    sessions: Arc<RwLock<HashMap<Uuid, GuestSession>>>,
    ops_state: Arc<RwLock<OpsState>>,
    storage: Storage,
    assets: AssetService,
    public_assets: Arc<public_assets::PublicAssetCatalog>,
    tx: broadcast::Sender<StreamPayload>,
}

fn asset_generation_disabled() -> bool {
    match std::env::var("ASSET_GENERATION_DISABLED") {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => true,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuestSession {
    session_id: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct GuestSessionResponse {
    session_id: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct BootstrapResponse {
    ws_path: String,
    tick_rate_hz: u64,
    snapshot: StreamPayload,
}

#[derive(Debug, Deserialize)]
struct MetricsQuery {
    since_tick: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    cursor: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ExplainQuery {
    agent_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct DepartmentStateQuery {
    floor_id: String,
}

#[derive(Debug, Deserialize)]
struct PolicySetRequest {
    key: String,
    value: f32,
    ttl_s: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SignalBroadcastRequest {
    signal_type: String,
    target_zone: String,
    intensity: f32,
    duration_s: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload")]
enum ClientWsCommand {
    #[serde(rename = "view.subscribe")]
    ViewSubscribe {
        channels: Vec<String>,
        agent_filter: Option<Uuid>,
        zone_filter: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct ClientViewFilter {
    channels: HashSet<String>,
    agent_filter: Option<Uuid>,
    zone_filter: Option<String>,
}

impl Default for ClientViewFilter {
    fn default() -> Self {
        Self {
            channels: HashSet::from([
                "tower_state".to_string(),
                "agent_events".to_string(),
                "alerts".to_string(),
                "metrics".to_string(),
            ]),
            agent_filter: None,
            zone_filter: None,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "agent_tower_sim=debug,tower_http=info".to_string()),
        )
        .init();

    let (tx, _) = broadcast::channel(2048);
    let storage = Storage::initialize().await;
    let mut initial_world = storage
        .load_latest_snapshot(DEFAULT_TOWER_ID)
        .await
        .unwrap_or_else(|| initialize_tower(1337));
    ensure_floor_tilemaps(&mut initial_world);
    let hydrated_metrics = storage.load_recent_metrics(DEFAULT_TOWER_ID, 2000).await;
    let hydrated_events = storage.load_recent_events(DEFAULT_TOWER_ID, 5000).await;
    let hydrated_ops_state = storage.load_ops_state(DEFAULT_TOWER_ID).await;
    info!(
        "storage backends: postgres={}, redis={}",
        storage.postgres_enabled(),
        storage.redis_enabled()
    );
    let public_assets = Arc::new(public_assets::PublicAssetCatalog::load("assets/public/v1").await);
    let state = AppState {
        world: Arc::new(Mutex::new(initial_world)),
        metrics: Arc::new(RwLock::new(hydrated_metrics)),
        events: Arc::new(RwLock::new(hydrated_events)),
        decisions: Arc::new(RwLock::new(HashMap::new())),
        sessions: Arc::new(RwLock::new(HashMap::new())),
        ops_state: Arc::new(RwLock::new(hydrated_ops_state)),
        storage,
        assets: AssetService::new("assets/generated"),
        public_assets,
        tx,
    };

    spawn_tick_loop(state.clone());

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/healthz", get(health_handler))
        .route("/session/guest", post(create_guest_session))
        .route("/tower/{id}/bootstrap", get(bootstrap_handler))
        .route("/tower/{id}/metrics", get(metrics_handler))
        .route("/tower/{id}/events", get(events_handler))
        .route("/tower/{id}/explain", get(explain_handler))
        .route(
            "/tower/{id}/departments/{dept}/state",
            get(department_state_handler),
        )
        .route("/tower/{id}/prestige", get(prestige_handler))
        .route("/tower/{id}/rankings", get(rankings_handler))
        .route("/events/openclaw", post(openclaw_event_handler))
        .route("/assets/generate", post(generate_asset_handler))
        .route("/assets/{id}/manifest", get(get_asset_manifest_handler))
        .route(
            "/assets/public/v1/catalog",
            get(public_asset_catalog_handler),
        )
        .route(
            "/assets/public/v1/tiles/{tile_id}",
            get(public_asset_tile_handler),
        )
        .route("/tower/{id}/policy", post(policy_set_handler))
        .route("/tower/{id}/signal", post(signal_broadcast_handler))
        .route("/ws/tower/{id}", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("bind server");
    info!("server listening on 0.0.0.0:3000");
    axum::serve(listener, app).await.expect("serve");
}

fn spawn_tick_loop(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous_runtime_assets: Option<RuntimeAssetMappings> = None;
        let mut previous_floor_placements: Option<RuntimeFloorPlacements> = None;

        loop {
            ticker.tick().await;
            let start = Instant::now();

            let (mut result, world_snapshot): (TickResult, TowerState) = {
                let mut world = state.world.lock().await;
                let result = run_tick(&mut world);
                (result, world.clone())
            };
            let current_runtime_assets = build_runtime_asset_mappings_with_previous(
                &world_snapshot,
                previous_runtime_assets.as_ref(),
            );
            result.diff.runtime_assets_delta = runtime_assets_delta_for_diff(
                previous_runtime_assets.as_ref(),
                &current_runtime_assets,
            );
            previous_runtime_assets = Some(current_runtime_assets.clone());
            let current_floor_placements = build_runtime_floor_placements_with_previous(
                &world_snapshot,
                previous_floor_placements.as_ref(),
            );
            result.diff.runtime_floor_placements_delta = runtime_floor_placements_delta_for_diff(
                previous_floor_placements.as_ref(),
                &current_floor_placements,
            );
            previous_floor_placements = Some(current_floor_placements.clone());

            {
                let mut metrics = state.metrics.write().await;
                metrics.push(result.metrics.clone());
                if metrics.len() > 10_000 {
                    let _ = metrics.drain(0..1000);
                }
            }
            {
                let mut events = state.events.write().await;
                events.extend(result.diff.new_events.iter().cloned());
                if events.len() > 20_000 {
                    let _ = events.drain(0..2000);
                }
            }
            {
                let mut decisions = state.decisions.write().await;
                for (id, trace) in result.decision_traces {
                    decisions.insert(id, trace);
                }
            }
            state
                .storage
                .persist_tick(
                    &world_snapshot,
                    &result.metrics,
                    &result.diff.new_events,
                    world_snapshot.tick % 30 == 0,
                )
                .await;

            if state
                .tx
                .send(StreamPayload::WorldDiff(result.diff))
                .is_err()
            {
                // no receivers; ignore
            }
            for alert in result.alerts {
                if state.tx.send(StreamPayload::AlertRaised(alert)).is_err() {
                    // no receivers; ignore
                }
            }
            if world_snapshot.tick % 5 == 0 {
                let snapshot = StreamPayload::WorldSnapshot {
                    tick: world_snapshot.tick,
                    tower: world_snapshot.tower_id.clone(),
                    floors: world_snapshot.floors.clone(),
                    agents: world_snapshot.agents.clone(),
                    economy: world_snapshot.economy.clone(),
                    alerts: world_snapshot.alerts.clone(),
                    runtime_assets: current_runtime_assets.clone(),
                    runtime_floor_placements: current_floor_placements.clone(),
                };
                if state.tx.send(snapshot).is_err() {
                    // no receivers; ignore
                }
            }

            let elapsed = start.elapsed();
            if elapsed > Duration::from_millis(200) {
                error!("tick overrun: {:?}", elapsed);
            }
        }
    });
}

async fn health_handler() -> impl IntoResponse {
    StatusCode::OK
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

fn build_world_snapshot(world: &TowerState) -> StreamPayload {
    let runtime_assets = build_runtime_asset_mappings(world);
    let runtime_floor_placements = build_runtime_floor_placements(world);
    StreamPayload::WorldSnapshot {
        tick: world.tick,
        tower: world.tower_id.clone(),
        floors: world.floors.clone(),
        agents: world.agents.clone(),
        economy: world.economy.clone(),
        alerts: world.alerts.clone(),
        runtime_assets,
        runtime_floor_placements,
    }
}

const ACTIVE_DESK_PROP_IDS: [&str; 5] = [
    "prop_push_desk",
    "prop_pull_desk",
    "prop_commit_desk",
    "prop_review_desk",
    "prop_test_desk",
];
// Layout/readability defaults for ASCII floorplans.
// We treat one tile as ~3ft and enforce circulation + density budgets.
const OFFICE_TILE_FEET: u8 = 3;
const OFFICE_MIN_AISLE_WIDTH_TILES: u8 = 1;
const OFFICE_MAIN_CORRIDOR_WIDTH_TILES: u8 = 2;
const OFFICE_TURNING_POCKET_SIZE_TILES: u8 = 2;
const OFFICE_PROP_MIN_SPACING_TILES: u8 = 2;
const OFFICE_AGENT_MIN_SPACING_TILES: u8 = 1;
const AGENT_VISUAL_HYSTERESIS_CADENCE_TICKS: u64 = 5;
const AGENT_REPOSITION_CADENCE_TICKS: u64 = 20;
const AGENT_FLOOR_TRANSITION_CADENCE_TICKS: u64 = 20;
const AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TICK: usize = 3;
const AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK: usize = 2;

fn build_runtime_asset_mappings(world: &TowerState) -> RuntimeAssetMappings {
    build_runtime_asset_mappings_with_previous(world, None)
}

fn build_runtime_asset_mappings_with_previous(
    world: &TowerState,
    previous: Option<&RuntimeAssetMappings>,
) -> RuntimeAssetMappings {
    let previous_agent_assets: HashMap<&str, &str> = previous
        .map(|mappings| {
            mappings
                .agents
                .iter()
                .map(|entry| (entry.entity_id.as_str(), entry.asset_id.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let mut agent_bindings = Vec::with_capacity(world.agents.len());
    let mut agents_per_floor: HashMap<Uuid, usize> = HashMap::new();
    for agent in &world.agents {
        *agents_per_floor.entry(agent.location_floor_id).or_default() += 1;
        let role = if agent.role.trim().is_empty() {
            "agent"
        } else {
            agent.role.as_str()
        };
        let agent_id = agent.id.to_string();
        let raw_asset_id = map_agent_asset_id(agent);
        let sticky_asset_id = map_agent_asset_id_with_hysteresis(
            agent,
            raw_asset_id,
            previous_agent_assets.get(agent_id.as_str()).copied(),
            world.tick,
        );
        agent_bindings.push(RuntimeAssetBinding {
            entity_id: agent_id,
            entity_kind: "agent".to_string(),
            label: role.to_string(),
            asset_id: sticky_asset_id.to_string(),
        });
    }

    let mut prop_bindings = Vec::with_capacity(world.floors.len());
    let mut fx_bindings = Vec::with_capacity(world.floors.len() + 1);
    for (idx, floor) in world.floors.iter().enumerate() {
        let assigned_agents = agents_per_floor.get(&floor.id).copied().unwrap_or_default();
        prop_bindings.push(RuntimeAssetBinding {
            entity_id: floor.id.to_string(),
            entity_kind: "prop".to_string(),
            label: format!("{} | {}", floor.name, floor_type_label(floor.floor_type)),
            asset_id: map_floor_prop_asset_id(floor.floor_type, world.tick, idx).to_string(),
        });
        fx_bindings.push(RuntimeAssetBinding {
            entity_id: format!("{}:fx", floor.id),
            entity_kind: "fx".to_string(),
            label: format!("{} fx", floor.name),
            asset_id: map_floor_fx_asset_id(assigned_agents, floor.capacity).to_string(),
        });
    }
    fx_bindings.push(RuntimeAssetBinding {
        entity_id: "tower_fx_global".to_string(),
        entity_kind: "fx".to_string(),
        label: "tower fx".to_string(),
        asset_id: map_global_fx_asset_id(world).to_string(),
    });

    let ui_state_asset = map_ui_state_asset_id(world.alerts.len());
    let ui_bindings = vec![
        RuntimeAssetBinding {
            entity_id: "tower_ui_health".to_string(),
            entity_kind: "ui".to_string(),
            label: "tower health".to_string(),
            asset_id: ui_state_asset.to_string(),
        },
        RuntimeAssetBinding {
            entity_id: "tower_ui_prestige".to_string(),
            entity_kind: "ui".to_string(),
            label: "tower prestige".to_string(),
            asset_id: "ui_prestige_star".to_string(),
        },
        RuntimeAssetBinding {
            entity_id: "tower_ui_probe".to_string(),
            entity_kind: "ui".to_string(),
            label: "runtime probe".to_string(),
            asset_id: "openai_subscription_probe".to_string(),
        },
    ];

    RuntimeAssetMappings {
        agents: agent_bindings,
        props: prop_bindings,
        ui: ui_bindings,
        fx: fx_bindings,
    }
}

fn runtime_assets_delta_for_diff(
    previous: Option<&RuntimeAssetMappings>,
    current: &RuntimeAssetMappings,
) -> RuntimeAssetMappingDeltas {
    let Some(previous) = previous else {
        return RuntimeAssetMappingDeltas {
            upsert_agents: current.agents.clone(),
            upsert_props: current.props.clone(),
            upsert_ui: current.ui.clone(),
            upsert_fx: current.fx.clone(),
            remove_agent_ids: vec![],
            remove_prop_ids: vec![],
            remove_ui_ids: vec![],
            remove_fx_ids: vec![],
        };
    };

    RuntimeAssetMappingDeltas {
        upsert_agents: runtime_asset_upserts(&previous.agents, &current.agents),
        upsert_props: runtime_asset_upserts(&previous.props, &current.props),
        upsert_ui: runtime_asset_upserts(&previous.ui, &current.ui),
        upsert_fx: runtime_asset_upserts(&previous.fx, &current.fx),
        remove_agent_ids: runtime_asset_removals(&previous.agents, &current.agents),
        remove_prop_ids: runtime_asset_removals(&previous.props, &current.props),
        remove_ui_ids: runtime_asset_removals(&previous.ui, &current.ui),
        remove_fx_ids: runtime_asset_removals(&previous.fx, &current.fx),
    }
}

fn runtime_asset_upserts(
    previous: &[RuntimeAssetBinding],
    current: &[RuntimeAssetBinding],
) -> Vec<RuntimeAssetBinding> {
    let previous_by_id: HashMap<&str, &RuntimeAssetBinding> = previous
        .iter()
        .map(|entry| (entry.entity_id.as_str(), entry))
        .collect();
    current
        .iter()
        .filter(|entry| {
            previous_by_id
                .get(entry.entity_id.as_str())
                .is_none_or(|old| !runtime_asset_visual_eq(old, entry))
        })
        .cloned()
        .collect()
}

fn runtime_asset_visual_eq(previous: &RuntimeAssetBinding, current: &RuntimeAssetBinding) -> bool {
    previous.entity_kind == current.entity_kind && previous.asset_id == current.asset_id
}

fn runtime_asset_removals(
    previous: &[RuntimeAssetBinding],
    current: &[RuntimeAssetBinding],
) -> Vec<String> {
    let current_ids: HashSet<&str> = current
        .iter()
        .map(|entry| entry.entity_id.as_str())
        .collect();
    previous
        .iter()
        .filter(|entry| !current_ids.contains(entry.entity_id.as_str()))
        .map(|entry| entry.entity_id.clone())
        .collect()
}

fn build_runtime_floor_placements(world: &TowerState) -> RuntimeFloorPlacements {
    build_runtime_floor_placements_with_previous(world, None)
}

fn build_runtime_floor_placements_with_previous(
    world: &TowerState,
    previous: Option<&RuntimeFloorPlacements>,
) -> RuntimeFloorPlacements {
    let valid_floors: HashSet<Uuid> = world.floors.iter().map(|f| f.id).collect();
    let previous_agent_floor: HashMap<String, Uuid> = previous
        .map(|placements| {
            let mut by_agent = HashMap::new();
            for floor in &placements.floors {
                let Ok(floor_id) = Uuid::parse_str(&floor.floor_id) else {
                    continue;
                };
                for agent in &floor.agents {
                    by_agent.insert(agent.entity_id.clone(), floor_id);
                }
            }
            by_agent
        })
        .unwrap_or_default();
    let previous_agent_state: HashMap<String, RuntimePlacedAsset> = previous
        .map(|placements| {
            let mut by_agent = HashMap::new();
            for floor in &placements.floors {
                for agent in &floor.agents {
                    by_agent.insert(agent.entity_id.clone(), agent.clone());
                }
            }
            by_agent
        })
        .unwrap_or_default();

    let mut optional_transition_candidates: Vec<&sim::Agent> = world
        .agents
        .iter()
        .filter(|agent| {
            let agent_id = agent.id.to_string();
            previous_agent_floor
                .get(agent_id.as_str())
                .copied()
                .filter(|prev| valid_floors.contains(prev))
                .is_some_and(|prev| {
                    prev != agent.location_floor_id
                        && should_allow_optional_floor_transition(agent, world.tick)
                })
        })
        .collect();
    optional_transition_candidates.sort_by(|a, b| {
        let a_urgency = a
            .mood
            .min(i32::from(a.needs.energy))
            .min(i32::from(a.needs.hunger));
        let b_urgency = b
            .mood
            .min(i32::from(b.needs.energy))
            .min(i32::from(b.needs.hunger));
        a_urgency
            .cmp(&b_urgency)
            .then_with(|| {
                jitter_score(agent_seed(a, 6), world.tick as usize)
                    .cmp(&jitter_score(agent_seed(b, 6), world.tick as usize))
            })
            .then_with(|| a.id.as_u128().cmp(&b.id.as_u128()))
    });
    let mut optional_transition_allowed: HashSet<Uuid> = HashSet::new();
    let mut optional_transition_count_by_target_floor: HashMap<Uuid, usize> = HashMap::new();
    for agent in optional_transition_candidates {
        if optional_transition_allowed.len() >= AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TICK {
            break;
        }
        let entry = optional_transition_count_by_target_floor
            .entry(agent.location_floor_id)
            .or_default();
        if *entry >= AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK {
            continue;
        }
        *entry += 1;
        optional_transition_allowed.insert(agent.id);
    }

    let mut agents_by_floor: HashMap<Uuid, Vec<&sim::Agent>> = HashMap::new();
    for agent in &world.agents {
        let agent_id = agent.id.to_string();
        let floor_id = match previous_agent_floor
            .get(agent_id.as_str())
            .copied()
            .filter(|prev| valid_floors.contains(prev))
        {
            Some(previous_floor_id) if previous_floor_id != agent.location_floor_id => {
                if requires_immediate_floor_transition(agent)
                    || optional_transition_allowed.contains(&agent.id)
                {
                    agent.location_floor_id
                } else {
                    previous_floor_id
                }
            }
            _ => agent.location_floor_id,
        };
        agents_by_floor.entry(floor_id).or_default().push(agent);
    }

    let mut floors = Vec::with_capacity(world.floors.len());
    for (idx, floor) in world.floors.iter().enumerate() {
        let width = floor.tilemap.width.max(1);
        let height = floor.tilemap.height.max(1);
        let floor_agents = agents_by_floor.get(&floor.id).cloned().unwrap_or_default();
        let mut base_cells = floor_walkable_cells(&floor.tilemap);
        if base_cells.is_empty() {
            base_cells.push((width / 2, height / 2));
        }
        let circulation_cells = reserved_circulation_cells(&floor.tilemap, &base_cells);

        let mut prop_assets = floor_prop_assets_for_placement(floor.floor_type, world.tick, idx);
        let prop_cap = max_props_for_floor(base_cells.len());
        if prop_assets.len() > prop_cap {
            prop_assets.truncate(prop_cap);
        }
        let upper_cells: Vec<(u8, u8)> = base_cells
            .iter()
            .copied()
            .filter(|(_, y)| *y <= height.saturating_div(2).max(1))
            .filter(|pos| {
                cell_clear_of_reserved(*pos, &circulation_cells, OFFICE_MIN_AISLE_WIDTH_TILES)
            })
            .collect();
        let circulation_safe_cells: Vec<(u8, u8)> = base_cells
            .iter()
            .copied()
            .filter(|pos| {
                cell_clear_of_reserved(*pos, &circulation_cells, OFFICE_MIN_AISLE_WIDTH_TILES)
            })
            .collect();
        let prop_candidates = if upper_cells.is_empty() {
            if circulation_safe_cells.is_empty() {
                base_cells.clone()
            } else {
                circulation_safe_cells
            }
        } else {
            upper_cells
        };
        let prop_seed =
            (world.tick << 20) ^ ((idx as u64) << 12) ^ (u64::from(width) << 6) ^ u64::from(height);
        let prop_slots = pick_cells_with_spacing(
            &prop_candidates,
            prop_assets.len(),
            OFFICE_PROP_MIN_SPACING_TILES,
            prop_seed,
        );
        let mut occupied: HashSet<(u8, u8)> = prop_slots.iter().copied().collect();
        let mut prop_placements = Vec::with_capacity(prop_assets.len());
        for (slot, (asset_id, (x, y))) in prop_assets.iter().zip(prop_slots.iter()).enumerate() {
            prop_placements.push(RuntimePlacedAsset {
                entity_id: format!("{}:prop:{slot}", floor.id),
                entity_kind: "prop".to_string(),
                label: format!("{} prop slot {}", floor.name, slot + 1),
                asset_id: (*asset_id).to_string(),
                x: *x,
                y: *y,
            });
        }

        let mut agent_placements = Vec::with_capacity(floor_agents.len());
        let agent_cells: Vec<(u8, u8)> = base_cells
            .iter()
            .copied()
            .filter(|pos| !occupied.contains(pos))
            .collect();
        let agent_candidates = if agent_cells.is_empty() {
            base_cells.clone()
        } else {
            agent_cells
        };
        let visible_agent_limit = max_visible_agents_for_floor(floor.floor_type, base_cells.len());
        let mut ordered_floor_agents: Vec<(&sim::Agent, Option<(u8, u8)>)> = floor_agents
            .iter()
            .copied()
            .map(|agent| {
                let agent_id = agent.id.to_string();
                let previous_pos = previous_agent_state
                    .get(agent_id.as_str())
                    .filter(|_| {
                        previous_agent_floor
                            .get(agent_id.as_str())
                            .is_some_and(|f| *f == floor.id)
                    })
                    .map(|entry| (entry.x, entry.y));
                (agent, previous_pos)
            })
            .collect();
        ordered_floor_agents.sort_by(|(a, a_prev), (b, b_prev)| {
            b_prev
                .is_some()
                .cmp(&a_prev.is_some())
                .then_with(|| {
                    a_prev
                        .unwrap_or((u8::MAX, u8::MAX))
                        .cmp(&b_prev.unwrap_or((u8::MAX, u8::MAX)))
                })
                .then_with(|| agent_seed(a, 4).cmp(&agent_seed(b, 4)))
                .then_with(|| a.id.as_u128().cmp(&b.id.as_u128()))
        });
        if ordered_floor_agents.len() > visible_agent_limit {
            ordered_floor_agents.truncate(visible_agent_limit);
        }
        for (agent, previous_pos_raw) in ordered_floor_agents.into_iter() {
            let agent_id = agent.id.to_string();
            let preferred_row = preferred_agent_row(agent.current_goal, height);
            let desired = pick_agent_cell(
                &agent_candidates,
                &occupied,
                preferred_row,
                agent,
                OFFICE_AGENT_MIN_SPACING_TILES,
            );
            let previous_pos = previous_pos_raw
                .filter(|pos| agent_candidates.contains(pos))
                .filter(|pos| !occupied.contains(pos));
            let (x, y) = match previous_pos {
                Some(prev) if !should_reposition_agent(agent, world.tick, prev, desired) => prev,
                _ => desired,
            };
            let raw_asset = map_agent_asset_id(agent);
            let sticky_asset = map_agent_asset_id_with_hysteresis(
                agent,
                raw_asset,
                previous_agent_state
                    .get(agent_id.as_str())
                    .map(|entry| entry.asset_id.as_str()),
                world.tick,
            );
            occupied.insert((x, y));
            agent_placements.push(RuntimePlacedAsset {
                entity_id: agent_id,
                entity_kind: "agent".to_string(),
                label: format!("{} | {}", agent.role, agent_goal_label(agent.current_goal)),
                asset_id: sticky_asset.to_string(),
                x,
                y,
            });
        }

        let assigned_agents = floor_agents.len();
        let hidden_agents = assigned_agents.saturating_sub(agent_placements.len());
        let floor_fx_asset = map_floor_fx_asset_id(assigned_agents, floor.capacity);
        let focus = if !agent_placements.is_empty() {
            placement_centroid(&agent_placements)
        } else if !prop_placements.is_empty() {
            placement_centroid(&prop_placements)
        } else {
            (width / 2, height / 2)
        };
        let primary_fx = nearest_cell_to(&base_cells, focus).unwrap_or((width / 2, height / 2));
        let mut fx_placements = vec![RuntimePlacedAsset {
            entity_id: format!("{}:fx:floor", floor.id),
            entity_kind: "fx".to_string(),
            label: if hidden_agents > 0 {
                format!(
                    "{} ambient fx | hidden_agents:{}",
                    floor.name, hidden_agents
                )
            } else {
                format!("{} ambient fx", floor.name)
            },
            asset_id: floor_fx_asset.to_string(),
            x: primary_fx.0,
            y: primary_fx.1,
        }];
        if world.alerts.iter().any(|a| a.severity == "critical") {
            let alert_fx = farthest_cell_from(&base_cells, primary_fx)
                .unwrap_or((width.saturating_sub(2), 1.min(height.saturating_sub(1))));
            fx_placements.push(RuntimePlacedAsset {
                entity_id: format!("{}:fx:alert", floor.id),
                entity_kind: "fx".to_string(),
                label: format!("{} alert fx", floor.name),
                asset_id: map_global_fx_asset_id(world).to_string(),
                x: alert_fx.0,
                y: alert_fx.1,
            });
        }

        floors.push(RuntimeFloorPlacement {
            floor_id: floor.id.to_string(),
            floor_name: floor.name.clone(),
            width,
            height,
            props: prop_placements,
            agents: agent_placements,
            fx: fx_placements,
        });
    }

    RuntimeFloorPlacements { floors }
}

fn runtime_floor_placements_delta_for_diff(
    previous: Option<&RuntimeFloorPlacements>,
    current: &RuntimeFloorPlacements,
) -> RuntimeFloorPlacementDeltas {
    let Some(previous) = previous else {
        return RuntimeFloorPlacementDeltas {
            floors: current
                .floors
                .iter()
                .map(runtime_floor_full_to_delta)
                .collect(),
            removed_floor_ids: vec![],
        };
    };

    let previous_by_floor: HashMap<&str, &RuntimeFloorPlacement> = previous
        .floors
        .iter()
        .map(|floor| (floor.floor_id.as_str(), floor))
        .collect();
    let current_ids: HashSet<&str> = current
        .floors
        .iter()
        .map(|floor| floor.floor_id.as_str())
        .collect();

    let mut changed = Vec::new();
    for floor in &current.floors {
        let prior = previous_by_floor.get(floor.floor_id.as_str());
        match prior {
            None => changed.push(runtime_floor_full_to_delta(floor)),
            Some(previous_floor) => {
                if let Some(delta) = runtime_floor_delta(previous_floor, floor) {
                    changed.push(delta);
                }
            }
        }
    }
    let removed_floor_ids = previous
        .floors
        .iter()
        .filter(|floor| !current_ids.contains(floor.floor_id.as_str()))
        .map(|floor| floor.floor_id.clone())
        .collect();

    RuntimeFloorPlacementDeltas {
        floors: changed,
        removed_floor_ids,
    }
}

fn runtime_floor_full_to_delta(floor: &RuntimeFloorPlacement) -> RuntimeFloorPlacementDelta {
    RuntimeFloorPlacementDelta {
        floor_id: floor.floor_id.clone(),
        floor_name: floor.floor_name.clone(),
        width: floor.width,
        height: floor.height,
        upsert_props: floor.props.clone(),
        upsert_agents: floor.agents.clone(),
        upsert_fx: floor.fx.clone(),
        remove_prop_ids: vec![],
        remove_agent_ids: vec![],
        remove_fx_ids: vec![],
    }
}

fn runtime_floor_delta(
    previous: &RuntimeFloorPlacement,
    current: &RuntimeFloorPlacement,
) -> Option<RuntimeFloorPlacementDelta> {
    let upsert_props = runtime_entity_upserts(&previous.props, &current.props);
    let upsert_agents = runtime_entity_upserts(&previous.agents, &current.agents);
    let upsert_fx = runtime_entity_upserts(&previous.fx, &current.fx);
    let remove_prop_ids = runtime_entity_removals(&previous.props, &current.props);
    let remove_agent_ids = runtime_entity_removals(&previous.agents, &current.agents);
    let remove_fx_ids = runtime_entity_removals(&previous.fx, &current.fx);
    let meta_changed = previous.floor_name != current.floor_name
        || previous.width != current.width
        || previous.height != current.height;
    if !meta_changed
        && upsert_props.is_empty()
        && upsert_agents.is_empty()
        && upsert_fx.is_empty()
        && remove_prop_ids.is_empty()
        && remove_agent_ids.is_empty()
        && remove_fx_ids.is_empty()
    {
        return None;
    }

    Some(RuntimeFloorPlacementDelta {
        floor_id: current.floor_id.clone(),
        floor_name: current.floor_name.clone(),
        width: current.width,
        height: current.height,
        upsert_props,
        upsert_agents,
        upsert_fx,
        remove_prop_ids,
        remove_agent_ids,
        remove_fx_ids,
    })
}

fn runtime_entity_upserts(
    previous: &[RuntimePlacedAsset],
    current: &[RuntimePlacedAsset],
) -> Vec<RuntimePlacedAsset> {
    let previous_by_id: HashMap<&str, &RuntimePlacedAsset> = previous
        .iter()
        .map(|entry| (entry.entity_id.as_str(), entry))
        .collect();
    current
        .iter()
        .filter(|entry| {
            previous_by_id
                .get(entry.entity_id.as_str())
                .is_none_or(|old| !runtime_placed_asset_visual_eq(old, entry))
        })
        .cloned()
        .collect()
}

fn runtime_placed_asset_visual_eq(
    previous: &RuntimePlacedAsset,
    current: &RuntimePlacedAsset,
) -> bool {
    if previous.entity_kind != current.entity_kind
        || previous.x != current.x
        || previous.y != current.y
    {
        return false;
    }
    if previous.entity_kind == "agent" {
        return true;
    }
    previous.asset_id == current.asset_id
}

fn runtime_entity_removals(
    previous: &[RuntimePlacedAsset],
    current: &[RuntimePlacedAsset],
) -> Vec<String> {
    let current_ids: HashSet<&str> = current
        .iter()
        .map(|entry| entry.entity_id.as_str())
        .collect();
    previous
        .iter()
        .filter(|entry| !current_ids.contains(entry.entity_id.as_str()))
        .map(|entry| entry.entity_id.clone())
        .collect()
}

fn floor_prop_assets_for_placement(
    floor_type: FloorType,
    tick: u64,
    idx: usize,
) -> Vec<&'static str> {
    let len = ACTIVE_DESK_PROP_IDS.len();
    let cadence_tick = (tick / 8) as usize;
    let base = (cadence_tick + idx) % len;
    match floor_type {
        FloorType::Residential => vec![
            "prop_planning_board",
            ACTIVE_DESK_PROP_IDS[base],
            ACTIVE_DESK_PROP_IDS[(base + 1) % len],
        ],
        FloorType::Commercial => vec![
            ACTIVE_DESK_PROP_IDS[base],
            ACTIVE_DESK_PROP_IDS[(base + 1) % len],
            ACTIVE_DESK_PROP_IDS[(base + 2) % len],
        ],
        FloorType::Service => vec![
            ACTIVE_DESK_PROP_IDS[(base + 2) % len],
            "prop_planning_board",
            ACTIVE_DESK_PROP_IDS[(base + 3) % len],
        ],
        FloorType::Utility => vec!["prop_test_desk", ACTIVE_DESK_PROP_IDS[(base + 4) % len]],
    }
}

fn max_props_for_floor(walkable_tiles: usize) -> usize {
    (walkable_tiles / 18).max(1).min(4)
}

fn max_visible_agents_for_floor(floor_type: FloorType, walkable_tiles: usize) -> usize {
    let tile_sqft = usize::from(OFFICE_TILE_FEET) * usize::from(OFFICE_TILE_FEET);
    let target_sqft_per_visible_agent = match floor_type {
        FloorType::Residential => 54,
        FloorType::Service => 45,
        FloorType::Commercial => 36,
        FloorType::Utility => 45,
    };
    let tiles_per_agent = (target_sqft_per_visible_agent / tile_sqft).max(1);
    let by_area = (walkable_tiles / tiles_per_agent).max(1);
    by_area.min(walkable_tiles.max(1))
}

fn reserved_circulation_cells(
    tilemap: &sim::FloorTileMap,
    walkable_cells: &[(u8, u8)],
) -> HashSet<(u8, u8)> {
    let width = tilemap.width;
    let height = tilemap.height;
    if width == 0 || height == 0 || walkable_cells.is_empty() {
        return HashSet::new();
    }
    let walkable: HashSet<(u8, u8)> = walkable_cells.iter().copied().collect();
    let mut reserved = HashSet::new();

    let col_start = width.saturating_sub(OFFICE_MAIN_CORRIDOR_WIDTH_TILES) / 2;
    let col_end = col_start
        .saturating_add(OFFICE_MAIN_CORRIDOR_WIDTH_TILES)
        .min(width);
    for x in col_start..col_end {
        for y in 0..height {
            if walkable.contains(&(x, y)) {
                reserved.insert((x, y));
            }
        }
    }

    let row_start = height.saturating_sub(OFFICE_MAIN_CORRIDOR_WIDTH_TILES) / 2;
    let row_end = row_start
        .saturating_add(OFFICE_MAIN_CORRIDOR_WIDTH_TILES)
        .min(height);
    for y in row_start..row_end {
        for x in 0..width {
            if walkable.contains(&(x, y)) {
                reserved.insert((x, y));
            }
        }
    }

    let pocket = OFFICE_TURNING_POCKET_SIZE_TILES.max(2);
    let pocket_anchors = [
        (col_start.saturating_sub(1), row_start.saturating_sub(1)),
        (
            col_start.saturating_sub(1),
            height.saturating_sub(1 + pocket),
        ),
    ];
    for (start_x, start_y) in pocket_anchors {
        let end_x = start_x.saturating_add(pocket).min(width);
        let end_y = start_y.saturating_add(pocket).min(height);
        for y in start_y..end_y {
            for x in start_x..end_x {
                if walkable.contains(&(x, y)) {
                    reserved.insert((x, y));
                }
            }
        }
    }

    reserved
}

fn cell_clear_of_reserved(
    cell: (u8, u8),
    reserved: &HashSet<(u8, u8)>,
    min_clearance_tiles: u8,
) -> bool {
    if reserved.is_empty() {
        return true;
    }
    reserved
        .iter()
        .all(|other| manhattan_distance(*other, cell) > u16::from(min_clearance_tiles))
}

fn pick_cells_with_spacing(
    candidates: &[(u8, u8)],
    count: usize,
    min_spacing_tiles: u8,
    seed: u64,
) -> Vec<(u8, u8)> {
    if count == 0 || candidates.is_empty() {
        return vec![];
    }
    let mut ordered = candidates.to_vec();
    ordered.sort_unstable();
    ordered.dedup();
    ordered.sort_by_key(|(x, y)| {
        let slot = usize::from(*y) * 257 + usize::from(*x) * 131;
        (jitter_score(seed, slot), *y, *x)
    });
    if ordered.is_empty() {
        return vec![];
    }

    let mut best = Vec::new();
    for spacing in (0..=min_spacing_tiles).rev() {
        let mut selected = Vec::with_capacity(count);
        for cell in ordered.iter().copied() {
            if selected
                .iter()
                .all(|prior| manhattan_distance(*prior, cell) >= u16::from(spacing))
            {
                selected.push(cell);
                if selected.len() >= count {
                    return selected;
                }
            }
        }
        if selected.len() > best.len() {
            best = selected;
        }
    }
    best
}

fn floor_walkable_cells(tilemap: &sim::FloorTileMap) -> Vec<(u8, u8)> {
    const WALL_CLEARANCE_TILES: u8 = 1;
    let width = tilemap.width;
    let height = tilemap.height;
    let mut interior = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let in_interior = (width <= 2 || (x > 0 && x + 1 < width))
                && (height <= 2 || (y > 0 && y + 1 < height));
            if in_interior
                && floor_tile_is_walkable_with_clearance(tilemap, x, y, WALL_CLEARANCE_TILES)
            {
                interior.push((x, y));
            }
        }
    }
    if !interior.is_empty() {
        return interior;
    }

    let mut any_walkable = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if floor_tile_is_walkable(tilemap, x, y) {
                any_walkable.push((x, y));
            }
        }
    }
    any_walkable
}

fn floor_tile_at(tilemap: &sim::FloorTileMap, x: u8, y: u8) -> Option<&str> {
    if tilemap.width == 0 || tilemap.height == 0 || x >= tilemap.width || y >= tilemap.height {
        return None;
    }
    let idx = usize::from(y) * usize::from(tilemap.width) + usize::from(x);
    tilemap.tiles.get(idx).map(String::as_str)
}

fn floor_tile_is_walkable(tilemap: &sim::FloorTileMap, x: u8, y: u8) -> bool {
    floor_tile_at(tilemap, x, y).is_some_and(|tile| !tile.starts_with("tile_wall_"))
}

fn floor_tile_is_walkable_with_clearance(
    tilemap: &sim::FloorTileMap,
    x: u8,
    y: u8,
    clearance_tiles: u8,
) -> bool {
    if !floor_tile_is_walkable(tilemap, x, y) {
        return false;
    }
    let width = tilemap.width;
    let height = tilemap.height;
    if width == 0 || height == 0 {
        return false;
    }
    let x_min = x.saturating_sub(clearance_tiles);
    let y_min = y.saturating_sub(clearance_tiles);
    let x_max = x.saturating_add(clearance_tiles).min(width.saturating_sub(1));
    let y_max = y.saturating_add(clearance_tiles).min(height.saturating_sub(1));
    for ny in y_min..=y_max {
        for nx in x_min..=x_max {
            if floor_tile_at(tilemap, nx, ny).is_some_and(|tile| tile.starts_with("tile_wall_")) {
                return false;
            }
        }
    }
    true
}

fn preferred_agent_row(goal: AgentGoal, height: u8) -> u8 {
    let min_y = if height > 2 { 1 } else { 0 };
    let max_y = if height > 2 {
        height.saturating_sub(2)
    } else {
        height.saturating_sub(1)
    };
    let mid_y = ((u16::from(min_y) + u16::from(max_y)) / 2) as u8;
    match goal {
        AgentGoal::Work => (mid_y + 1).min(max_y),
        AgentGoal::Eat => mid_y,
        AgentGoal::Rest => max_y,
        AgentGoal::Socialize => mid_y.saturating_sub(1).max(min_y),
        AgentGoal::Relocate => ((u16::from(mid_y) + u16::from(max_y)) / 2) as u8,
    }
}

fn pick_agent_cell(
    candidates: &[(u8, u8)],
    occupied: &HashSet<(u8, u8)>,
    preferred_row: u8,
    agent: &sim::Agent,
    min_spacing_tiles: u8,
) -> (u8, u8) {
    if candidates.is_empty() {
        return (0, 0);
    }
    let seed = agent_seed(agent, 0);
    let mut best_any: Option<((u8, u16, u64, u8, u8), (u8, u8))> = None;
    let mut best_free: Option<((u8, u16, u64, u8, u8), (u8, u8))> = None;
    let mut best_free_clear: Option<((u8, u16, u64, u8, u8), (u8, u8))> = None;

    for (slot, (x, y)) in candidates.iter().copied().enumerate() {
        let crowding = occupied
            .iter()
            .filter(|prior| manhattan_distance(**prior, (x, y)) <= 2)
            .count() as u16;
        let key = (
            y.abs_diff(preferred_row),
            crowding,
            jitter_score(seed, slot),
            x,
            y,
        );
        if best_any.as_ref().is_none_or(|v| key < v.0) {
            best_any = Some((key, (x, y)));
        }
        if !occupied.contains(&(x, y)) {
            if best_free.as_ref().is_none_or(|v| key < v.0) {
                best_free = Some((key, (x, y)));
            }
            let clear_of_neighbors = occupied
                .iter()
                .all(|prior| manhattan_distance(*prior, (x, y)) > u16::from(min_spacing_tiles));
            if clear_of_neighbors && best_free_clear.as_ref().is_none_or(|v| key < v.0) {
                best_free_clear = Some((key, (x, y)));
            }
        }
    }

    best_free_clear
        .or(best_free)
        .or(best_any)
        .map(|(_, pos)| pos)
        .unwrap_or((0, 0))
}

fn should_reposition_agent(
    agent: &sim::Agent,
    tick: u64,
    previous: (u8, u8),
    desired: (u8, u8),
) -> bool {
    if previous == desired {
        return false;
    }
    if matches!(agent.current_goal, AgentGoal::Relocate) {
        return true;
    }
    (agent_seed(agent, 0) + tick) % AGENT_REPOSITION_CADENCE_TICKS == 0
}

fn requires_immediate_floor_transition(agent: &sim::Agent) -> bool {
    matches!(agent.current_goal, AgentGoal::Relocate)
        || agent.mood <= 20
        || agent.needs.energy <= 8
        || agent.needs.hunger <= 8
}

fn should_allow_optional_floor_transition(agent: &sim::Agent, tick: u64) -> bool {
    !requires_immediate_floor_transition(agent)
        && (agent_seed(agent, 2) + tick) % AGENT_FLOOR_TRANSITION_CADENCE_TICKS == 0
}

#[cfg(test)]
fn should_transition_agent_floor(agent: &sim::Agent, tick: u64) -> bool {
    requires_immediate_floor_transition(agent)
        || should_allow_optional_floor_transition(agent, tick)
}

fn agent_seed(agent: &sim::Agent, index: usize) -> u64 {
    let raw = agent.id.as_u128();
    let low = raw as u64;
    let high = (raw >> 64) as u64;
    low ^ high ^ ((index as u64).rotate_left((index % 63) as u32))
}

fn jitter_score(seed: u64, slot: usize) -> u64 {
    let mut v = seed ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    v = v.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    v ^ (v >> 33)
}

fn placement_centroid(entries: &[RuntimePlacedAsset]) -> (u8, u8) {
    if entries.is_empty() {
        return (0, 0);
    }
    let (sum_x, sum_y) = entries.iter().fold((0u32, 0u32), |(sx, sy), entry| {
        (sx + u32::from(entry.x), sy + u32::from(entry.y))
    });
    let len = entries.len() as u32;
    ((sum_x / len) as u8, (sum_y / len) as u8)
}

fn nearest_cell_to(cells: &[(u8, u8)], target: (u8, u8)) -> Option<(u8, u8)> {
    cells
        .iter()
        .copied()
        .min_by_key(|cell| manhattan_distance(*cell, target))
}

fn farthest_cell_from(cells: &[(u8, u8)], anchor: (u8, u8)) -> Option<(u8, u8)> {
    cells
        .iter()
        .copied()
        .max_by_key(|cell| manhattan_distance(*cell, anchor))
}

fn manhattan_distance(a: (u8, u8), b: (u8, u8)) -> u16 {
    u16::from(a.0.abs_diff(b.0)) + u16::from(a.1.abs_diff(b.1))
}

fn agent_goal_label(goal: AgentGoal) -> &'static str {
    match goal {
        AgentGoal::Work => "work",
        AgentGoal::Eat => "eat",
        AgentGoal::Rest => "rest",
        AgentGoal::Socialize => "socialize",
        AgentGoal::Relocate => "relocate",
    }
}

fn floor_type_label(floor_type: FloorType) -> &'static str {
    match floor_type {
        FloorType::Residential => "residential",
        FloorType::Commercial => "commercial",
        FloorType::Service => "service",
        FloorType::Utility => "utility",
    }
}

fn map_agent_asset_id(agent: &sim::Agent) -> &'static str {
    let stressed = agent.mood <= 35 || agent.needs.energy <= 15 || agent.needs.hunger <= 15;
    if agent.role == "worker" {
        if stressed {
            "agent_worker_alert"
        } else if matches!(
            agent.current_goal,
            AgentGoal::Work | AgentGoal::Relocate | AgentGoal::Socialize
        ) {
            "agent_worker_move"
        } else {
            "agent_worker_idle"
        }
    } else if stressed {
        "agent_coder_fail"
    } else if matches!(agent.current_goal, AgentGoal::Work) {
        "agent_coder_work"
    } else {
        "agent_coder_idle"
    }
}

fn map_agent_asset_id_with_hysteresis(
    agent: &sim::Agent,
    raw_asset_id: &'static str,
    previous_asset_id: Option<&str>,
    tick: u64,
) -> &'static str {
    let Some(previous_raw) = previous_asset_id else {
        return raw_asset_id;
    };
    let Some(previous_asset_id) = normalize_agent_asset_id(previous_raw) else {
        return raw_asset_id;
    };
    if previous_asset_id == raw_asset_id {
        return raw_asset_id;
    }

    let worker = agent.role == "worker";
    let previous_is_worker = previous_asset_id.starts_with("agent_worker_");
    if worker != previous_is_worker {
        return raw_asset_id;
    }

    let severe = agent.mood <= 20 || agent.needs.energy <= 8 || agent.needs.hunger <= 8;
    let recovering = agent.mood >= 55 && agent.needs.energy >= 35 && agent.needs.hunger >= 35;

    if raw_asset_id == "agent_worker_alert" || raw_asset_id == "agent_coder_fail" {
        return raw_asset_id;
    }
    if (previous_asset_id == "agent_worker_alert" || previous_asset_id == "agent_coder_fail")
        && !recovering
    {
        return previous_asset_id;
    }
    if severe {
        return if worker {
            "agent_worker_alert"
        } else {
            "agent_coder_fail"
        };
    }

    if (agent_seed(agent, 1) + tick) % AGENT_VISUAL_HYSTERESIS_CADENCE_TICKS != 0 {
        return previous_asset_id;
    }
    raw_asset_id
}

fn normalize_agent_asset_id(asset_id: &str) -> Option<&'static str> {
    match asset_id {
        "agent_coder_idle" => Some("agent_coder_idle"),
        "agent_coder_work" => Some("agent_coder_work"),
        "agent_coder_fail" => Some("agent_coder_fail"),
        "agent_worker_idle" => Some("agent_worker_idle"),
        "agent_worker_move" => Some("agent_worker_move"),
        "agent_worker_alert" => Some("agent_worker_alert"),
        _ => None,
    }
}

fn map_floor_prop_asset_id(floor_type: FloorType, tick: u64, idx: usize) -> &'static str {
    let cadence_tick = (tick / 8) as usize;
    let cycle = ACTIVE_DESK_PROP_IDS[(cadence_tick + idx) % ACTIVE_DESK_PROP_IDS.len()];
    match floor_type {
        FloorType::Residential => "prop_planning_board",
        FloorType::Commercial => cycle,
        FloorType::Service => {
            ACTIVE_DESK_PROP_IDS[(cadence_tick + idx + 2) % ACTIVE_DESK_PROP_IDS.len()]
        }
        FloorType::Utility => "prop_test_desk",
    }
}

fn map_ui_state_asset_id(alert_count: usize) -> &'static str {
    if alert_count >= 25 {
        "ui_state_broken"
    } else if alert_count >= 15 {
        "ui_state_stalled"
    } else if alert_count >= 8 {
        "ui_state_degraded"
    } else {
        "ui_state_healthy"
    }
}

fn map_floor_fx_asset_id(assigned_agents: usize, capacity: u16) -> &'static str {
    if assigned_agents == 0 {
        return "fx_spark_flicker";
    }
    if capacity > 0 {
        let pct = (assigned_agents.saturating_mul(100)) / capacity as usize;
        if pct >= 95 {
            return "fx_fail_pop";
        }
    }
    "fx_upload_burst"
}

fn map_global_fx_asset_id(world: &TowerState) -> &'static str {
    if world.alerts.iter().any(|a| a.severity == "critical") {
        "fx_fail_pop"
    } else if !world.alerts.is_empty() {
        "fx_spark_flicker"
    } else {
        "fx_upload_burst"
    }
}

async fn create_guest_session(State(state): State<AppState>) -> impl IntoResponse {
    let session = GuestSession {
        session_id: Uuid::new_v4(),
        created_at: Utc::now(),
    };
    state
        .sessions
        .write()
        .await
        .insert(session.session_id, session.clone());
    Json(GuestSessionResponse {
        session_id: session.session_id,
        created_at: session.created_at,
    })
}

async fn bootstrap_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<BootstrapResponse>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let world = state.world.lock().await;
    let snapshot = build_world_snapshot(&world);
    Ok(Json(BootstrapResponse {
        ws_path: format!("/ws/tower/{}", world.tower_id),
        tick_rate_hz: 1,
        snapshot,
    }))
}

async fn metrics_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<Vec<MetricsPoint>>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let points = state.metrics.read().await;
    let filtered = points
        .iter()
        .filter(|m| query.since_tick.is_none_or(|s| m.tick >= s))
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(filtered))
}

async fn events_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<SimEvent>>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let events = state.events.read().await;
    let cursor = query.cursor.unwrap_or(0);
    let limit = query.limit.unwrap_or(200).min(1000);
    if cursor >= events.len() {
        return Ok(Json(vec![]));
    }
    let slice = events[cursor..events.len().min(cursor + limit)].to_vec();
    Ok(Json(slice))
}

async fn explain_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ExplainQuery>,
) -> Result<Json<StreamPayload>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let decisions = state.decisions.read().await;
    let trace = decisions
        .get(&query.agent_id)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(StreamPayload::AgentExplain {
        agent_id: query.agent_id,
        trace,
    }))
}

async fn department_state_handler(
    Path((tower_id, dept)): Path<(String, String)>,
    State(state): State<AppState>,
    Query(query): Query<DepartmentStateQuery>,
) -> Result<Json<sim::DepartmentStateUpdate>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let ops = state.ops_state.read().await;
    get_department_state(&ops, &tower_id, &query.floor_id, &dept)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn prestige_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<sim::FloorPrestige>>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let ops = state.ops_state.read().await;
    let mut values = compute_floor_prestige(&ops, Utc::now());
    values.retain(|p| p.tower_id == tower_id);
    Ok(Json(values))
}

async fn rankings_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<sim::RankingEntry>>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let ops = state.ops_state.read().await;
    Ok(Json(compute_rankings(&ops, &tower_id, Utc::now())))
}

async fn openclaw_event_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(mut event): Json<OpenClawEvent>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if event.department_id.is_empty() {
        event.department_id = CODING_DEPARTMENT.to_string();
    }
    if event.tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Ok(required_key) = std::env::var("OPENCLAW_INGEST_KEY") {
        let incoming = headers
            .get("x-openclaw-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if incoming != required_key {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let now = Utc::now();
    let outcome = {
        let mut ops = state.ops_state.write().await;
        process_event(&mut ops, event, now).map_err(|_| StatusCode::BAD_REQUEST)?
    };
    let ops_snapshot = state.ops_state.read().await.clone();
    state.storage.persist_ops_state(&ops_snapshot).await;

    {
        let mut events = state.events.write().await;
        events.push(outcome.sim_event);
        if events.len() > 20_000 {
            let _ = events.drain(0..2000);
        }
    }
    if let Some(desk) = outcome.desk_update.clone() {
        let _ = state.tx.send(StreamPayload::DeskActivity(desk));
    }
    let _ = state.tx.send(StreamPayload::DepartmentState(
        outcome.department_update.clone(),
    ));
    let _ = state
        .tx
        .send(StreamPayload::PrestigeUpdated(outcome.prestige.clone()));

    Ok(Json(serde_json::json!({
        "status": "ok",
        "department_state": outcome.department_update,
        "prestige": outcome.prestige
    })))
}

async fn generate_asset_handler(
    State(state): State<AppState>,
    Json(req): Json<GenerateAssetRequest>,
) -> impl IntoResponse {
    if asset_generation_disabled() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "disabled",
                "reason": "asset_generation_pipeline_disabled_v1",
                "message": "POST /assets/generate is disabled while we migrate to curated public sprite packs.",
                "hint": "Set ASSET_GENERATION_DISABLED=0 to re-enable the legacy generator temporarily."
            })),
        )
            .into_response();
    }

    match state.assets.generate(req).await {
        Ok(resp) => (StatusCode::OK, Json(serde_json::json!(resp))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "error": e,
            })),
        )
            .into_response(),
    }
}

async fn get_asset_manifest_handler(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<assets::AssetManifest>, StatusCode> {
    state
        .assets
        .get_manifest(id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

async fn public_asset_catalog_handler(
    State(state): State<AppState>,
) -> Json<public_assets::PublicAssetCatalogResponse> {
    Json(state.public_assets.to_response())
}

async fn public_asset_tile_handler(
    Path(tile_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !public_assets::is_safe_asset_id(&tile_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(tile_path) = state.public_assets.resolve_tile_path(&tile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::read(tile_path).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=600"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn policy_set_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<PolicySetRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let policy = {
        let mut world = state.world.lock().await;
        apply_policy(&mut world, req.key, req.value, req.ttl_s)
    };
    let _ = state.tx.send(StreamPayload::PolicySet(policy.clone()));
    Ok(Json(
        serde_json::json!({ "status": "ok", "policy": policy }),
    ))
}

async fn signal_broadcast_handler(
    Path(tower_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SignalBroadcastRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    let signal = {
        let mut world = state.world.lock().await;
        broadcast_signal(
            &mut world,
            req.signal_type,
            req.target_zone,
            req.intensity,
            req.duration_s,
        )
    };
    let _ = state
        .tx
        .send(StreamPayload::SignalBroadcast(signal.clone()));
    Ok(Json(
        serde_json::json!({ "status": "ok", "signal": signal }),
    ))
}

async fn ws_handler(
    Path(tower_id): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    if tower_id != DEFAULT_TOWER_ID {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(ws.on_upgrade(move |socket| ws_connection(socket, state)))
}

async fn ws_connection(mut socket: WebSocket, state: AppState) {
    let mut filter = ClientViewFilter::default();
    let world = state.world.lock().await.clone();
    let hello = StreamPayload::ServerHello {
        server_time: Utc::now(),
        tick_rate: 1,
        tower_id: world.tower_id.clone(),
    };
    if send_payload(&mut socket, &hello).await.is_err() {
        return;
    }
    let snapshot = build_world_snapshot(&world);
    if send_payload(&mut socket, &snapshot).await.is_err() {
        return;
    }

    let mut rx = state.tx.subscribe();
    loop {
        tokio::select! {
            outbound = rx.recv() => {
                match outbound {
                    Ok(payload) => {
                        if !should_send_payload(&payload, &filter) {
                            continue;
                        }
                        if send_payload(&mut socket, &payload).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(v))) => {
                        if socket.send(Message::Pong(v)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        handle_ws_command(text.as_str(), &mut filter);
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            handle_ws_command(text, &mut filter);
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn send_payload(socket: &mut WebSocket, payload: &StreamPayload) -> Result<(), ()> {
    let bytes = serde_json::to_vec(payload).map_err(|_| ())?;
    socket
        .send(Message::Binary(bytes.into()))
        .await
        .map_err(|_| ())
}

fn handle_ws_command(raw: &str, filter: &mut ClientViewFilter) {
    let parsed = serde_json::from_str::<ClientWsCommand>(raw);
    if let Ok(ClientWsCommand::ViewSubscribe {
        channels,
        agent_filter,
        zone_filter,
    }) = parsed
    {
        if !channels.is_empty() {
            filter.channels = channels.into_iter().collect();
        }
        filter.agent_filter = agent_filter;
        filter.zone_filter = zone_filter;
    }
}

fn should_send_payload(payload: &StreamPayload, filter: &ClientViewFilter) -> bool {
    let channel = match payload {
        StreamPayload::ServerHello { .. } => "tower_state",
        StreamPayload::WorldSnapshot { .. } => "tower_state",
        StreamPayload::WorldDiff(_) => "metrics",
        StreamPayload::AgentExplain { .. } => "agent_events",
        StreamPayload::AlertRaised(_) => "alerts",
        StreamPayload::SignalBroadcast(_) => "agent_events",
        StreamPayload::PolicySet(_) => "agent_events",
        StreamPayload::DeskActivity(_) => "agent_events",
        StreamPayload::DepartmentState(_) => "agent_events",
        StreamPayload::PrestigeUpdated(_) => "metrics",
    };
    if !filter.channels.contains(channel) {
        return false;
    }

    match payload {
        StreamPayload::AgentExplain { agent_id, .. } => {
            filter.agent_filter.is_none_or(|id| id == *agent_id)
        }
        StreamPayload::WorldDiff(diff) => {
            if let Some(agent_id) = filter.agent_filter {
                diff.changed_agents.contains(&agent_id)
            } else {
                true
            }
        }
        _ => true,
    }
}

const INDEX_HTML: &str = r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Agent Tower Sim</title>
  <style>
    :root {
      --bg: #000000;
      --panel: #030303;
      --ink: #ffffff;
      --muted: #b7b7b7;
      --danger: #ff8080;
    }
    body { margin: 0; font-family: "IBM Plex Mono", monospace; background: var(--bg); color: var(--ink); }
    header { padding: 14px 18px; border-bottom: 1px solid #1b1b1b; display:flex; justify-content:space-between; }
    .grid { display:grid; grid-template-columns: 2fr 1fr; gap: 14px; padding: 14px; }
    .panel { background: var(--panel); border: 1px solid #1f1f1f; border-radius: 10px; padding: 12px; min-height: 220px; }
    .tower { display:flex; flex-direction:column-reverse; gap:8px; }
    .floor { padding:8px; border-radius:8px; background:#050505; border:1px solid #1f1f1f; }
    .row { display:flex; justify-content:space-between; gap: 10px; }
    .metric { color: var(--muted); }
    .asset-status { margin-bottom: 8px; }
    .tilegrid { margin-top:6px; display:grid; gap:1px; width:max-content; padding:6px; border-radius:6px; border:1px solid #262626; background:#000; }
    .tilecell { width:24px; height:24px; background:#000; border:1px solid #090909; position:relative; overflow:hidden; display:flex; align-items:center; justify-content:center; }
    .tilecell.has-entity { outline: 1px solid #2f2f2f; outline-offset: -1px; }
    .tilecell.floor { background: #000; }
    .glyph-outline {
      text-shadow:
        -1px -1px 0 #000,
         1px -1px 0 #000,
        -1px  1px 0 #000,
         1px  1px 0 #000;
    }
    .tileglyph {
      font: 700 15px/1 "IBM Plex Mono", monospace;
      color:#dcdcdc;
      z-index:1;
      pointer-events:none;
    }
    .tileentity {
      position:absolute;
      inset:0;
      display:flex;
      align-items:center;
      justify-content:center;
      font:700 15px/1 "IBM Plex Mono", monospace;
      color:#fff;
      pointer-events:none;
      z-index:2;
      transform: translate(var(--entity-dx, 0px), var(--entity-dy, 0px));
      text-shadow:
        -1px -1px 0 #000,
         1px -1px 0 #000,
        -1px  1px 0 #000,
         1px  1px 0 #000;
    }
    .tileentity.prop { opacity: 0.95; z-index: 2; }
    .tileentity.agent { opacity: 1; z-index: 3; animation: agentBob 1.8s ease-in-out infinite; animation-delay: var(--entity-delay, 0s); }
    .tileentity.fx { opacity: 0.9; z-index: 4; animation: fxPulse 0.95s steps(2, end) infinite; animation-delay: var(--entity-delay, 0s); }
    .tileentity.prop { color:#bbbbbb; }
    .tileentity.agent { color:#ffffff; }
    .tileentity.fx { color:#d8d8d8; }
    .tilecrowd {
      position:absolute;
      right:1px;
      bottom:1px;
      z-index:5;
      font:700 9px/1 "IBM Plex Mono", monospace;
      color:#fff;
      background:#000;
      border:1px solid #202020;
      padding:0 1px;
      text-shadow:
        -1px -1px 0 #000,
         1px -1px 0 #000,
        -1px  1px 0 #000,
         1px  1px 0 #000;
      pointer-events:none;
    }
    @keyframes agentBob {
      0%, 100% { transform: translate(var(--entity-dx, 0px), var(--entity-dy, 0px)); }
      50% { transform: translate(var(--entity-dx, 0px), calc(var(--entity-dy, 0px) - 1px)); }
    }
    @keyframes fxPulse {
      0%, 100% { transform: translate(var(--entity-dx, 0px), var(--entity-dy, 0px)) scale(1); opacity: 0.75; }
      50% { transform: translate(var(--entity-dx, 0px), var(--entity-dy, 0px)) scale(1.06); opacity: 1; }
    }
    .sample-grid { margin-top:8px; display:grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap:6px; }
    .sample-card { border:1px solid #2a2a2a; border-radius:6px; background:#050505; padding:6px; display:flex; gap:6px; align-items:center; min-height:44px; }
    .sample-art { width:32px; height:32px; flex:0 0 32px; border:1px solid #1e1e1e; background:#000; display:flex; align-items:center; justify-content:center; color:#fff; font:700 16px/1 "IBM Plex Mono", monospace; }
    .sample-id { font:11px/1.15 "IBM Plex Mono", monospace; color:#d0d0d0; word-break:break-all; }
    .runtime-section { margin-top: 8px; }
    .runtime-group { margin-top: 8px; }
    .runtime-head { font:11px/1.2 "IBM Plex Mono", monospace; color:#b7b7b7; }
    .runtime-meta { font:10px/1.1 "IBM Plex Mono", monospace; color:#9c9c9c; word-break:break-all; margin-top:2px; }
    .alert { color: var(--danger); margin-bottom: 6px; }
    @media (max-width: 860px) { .grid { grid-template-columns: 1fr; } }
  </style>
</head>
<body>
  <header>
    <strong>Agent Tower — Living Dashboard</strong>
    <span id="tick">tick: 0</span>
  </header>
  <main class="grid">
    <section class="panel">
      <h3>Tower Floors</h3>
      <div id="floors" class="tower"></div>
    </section>
    <section class="panel">
      <h3>Economy + Alerts</h3>
      <div class="metric asset-status" id="assetStatus">render mode: ascii_v1</div>
      <div class="metric" id="economy"></div>
      <hr />
      <div id="alerts"></div>
      <hr />
      <div class="metric">Asset Samples</div>
      <div id="assetSamples" class="sample-grid"></div>
      <hr />
      <div class="metric">Runtime Entity Assets</div>
      <div id="runtimeAssets" class="runtime-section"></div>
    </section>
  </main>
  <script>
    const floorsEl = document.getElementById('floors');
    const tickEl = document.getElementById('tick');
    const economyEl = document.getElementById('economy');
    const alertsEl = document.getElementById('alerts');
    const assetStatusEl = document.getElementById('assetStatus');
    const assetSamplesEl = document.getElementById('assetSamples');
    const runtimeAssetsEl = document.getElementById('runtimeAssets');
    const alerts = [];
    let latestSnapshotPayload = null;

    function escapeHtml(value) {
      return String(value ?? '')
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');
    }

    function tileToken(tileId) {
      if (tileId === 'tile_floor_clean') return ' ';
      if (tileId === 'tile_wall_corner_top_left') return '⡏';
      if (tileId === 'tile_wall_corner_top_right') return '⢹';
      if (tileId === 'tile_wall_corner_bottom_left') return '⣇';
      if (tileId === 'tile_wall_corner_bottom_right') return '⣸';
      if (tileId === 'tile_wall_inner_corner_top_left') return '⢹';
      if (tileId === 'tile_wall_inner_corner_top_right') return '⡏';
      if (tileId === 'tile_wall_inner_corner_bottom_left') return '⣸';
      if (tileId === 'tile_wall_inner_corner_bottom_right') return '⣇';
      if (tileId === 'tile_wall_edge_top') return '⠉';
      if (tileId === 'tile_wall_edge_bottom') return '⠤';
      if (tileId === 'tile_wall_edge_left') return '⡇';
      if (tileId === 'tile_wall_edge_right') return '⢸';
      if (tileId === 'tile_wall_door_left') return '⠪';
      if (tileId === 'tile_wall_door_right') return '⠕';
      if (tileId === 'tile_wall_window_left') return '⡆';
      if (tileId === 'tile_wall_window_right') return '⢃';
      if (tileId === 'tile_wall_cross') return '⢼';
      if (tileId === 'tile_wall_terminal') return '⣾';
      if (tileId === 'tile_wall_pillar' || tileId === 'tile_wall_pillar_alt') return '⣿';
      if (tileId === 'tile_wall_t_top') return '⢸';
      if (tileId === 'tile_wall_t_bottom') return '⡇';
      if (tileId && tileId.startsWith('tile_wall_t_')) return '⢼';
      if (tileId && tileId.startsWith('tile_wall_')) return '⣿';
      return '?';
    }

    function inferEntityKind(assetId) {
      if (!assetId || typeof assetId !== 'string') return '';
      if (assetId.startsWith('agent_')) return 'agent';
      if (assetId.startsWith('prop_')) return 'prop';
      if (assetId.startsWith('fx_')) return 'fx';
      if (assetId.startsWith('ui_')) return 'ui';
      if (assetId.startsWith('portrait_')) return 'portrait';
      return '';
    }

    function entityToken(assetId, kind) {
      const resolvedKind = kind || inferEntityKind(assetId);
      if (typeof assetId === 'string') {
        if (assetId.startsWith('agent_')) {
          if (assetId.includes('alert') || assetId.includes('fail')) return '!';
          if (assetId.includes('move')) return '&';
          return '@';
        }
        if (assetId.startsWith('prop_')) {
          if (assetId.includes('desk')) return '=';
          if (assetId.includes('planning')) return 'B';
          return '+';
        }
        if (assetId.startsWith('fx_')) return '*';
        if (assetId.startsWith('ui_')) return 'U';
        if (assetId.startsWith('portrait_')) return ':';
        if (assetId.startsWith('tile_')) return tileToken(assetId);
      }
      if (resolvedKind === 'agent') return '@';
      if (resolvedKind === 'prop') return '=';
      if (resolvedKind === 'fx') return '*';
      if (resolvedKind === 'ui') return 'U';
      return '?';
    }

    function cellKey(x, y) {
      return `${x},${y}`;
    }

    function runtimeAssetMapById(runtimeAssets, groupKey) {
      const map = Object.create(null);
      if (!runtimeAssets || typeof runtimeAssets !== 'object') return map;
      const entries = Array.isArray(runtimeAssets[groupKey]) ? runtimeAssets[groupKey] : [];
      entries.forEach((entry) => {
        if (!entry || typeof entry.entity_id !== 'string') return;
        if (typeof entry.asset_id !== 'string') return;
        map[entry.entity_id] = entry.asset_id;
      });
      return map;
    }

    function buildPlacementCellMap(floorPlacement, width, height, runtimeAssets) {
      const byCell = Object.create(null);
      if (!floorPlacement || typeof floorPlacement !== 'object') {
        return byCell;
      }
      const agentAssetsById = runtimeAssetMapById(runtimeAssets, 'agents');
      const ingest = (entries, kind) => {
        if (!Array.isArray(entries)) return;
        entries.forEach((entry) => {
          const x = Number(entry?.x);
          const y = Number(entry?.y);
          if (!Number.isInteger(x) || !Number.isInteger(y)) return;
          if (x < 0 || y < 0 || x >= width || y >= height) return;
          const entityId = typeof entry?.entity_id === 'string' ? entry.entity_id : '';
          const fallbackAssetId = typeof entry?.asset_id === 'string' ? entry.asset_id : '';
          const resolvedAssetId = kind === 'agent' && entityId
            ? (agentAssetsById[entityId] || fallbackAssetId)
            : fallbackAssetId;
          const key = cellKey(x, y);
          if (!byCell[key]) byCell[key] = [];
          byCell[key].push({
            asset_id: resolvedAssetId,
            entity_kind: kind,
            label: typeof entry?.label === 'string' ? entry.label : '',
            entity_id: entityId,
            x,
            y,
          });
        });
      };
      ingest(floorPlacement.props, 'prop');
      ingest(floorPlacement.agents, 'agent');
      ingest(floorPlacement.fx, 'fx');
      return byCell;
    }

    function entityLayerRank(kind) {
      if (kind === 'prop') return 1;
      if (kind === 'fx') return 2;
      if (kind === 'agent') return 3;
      return 0;
    }

    function renderEntityOverlay(cellEntities) {
      if (!Array.isArray(cellEntities) || cellEntities.length === 0) return '';
      const layered = [...cellEntities].sort((a, b) => entityLayerRank(a.entity_kind) - entityLayerRank(b.entity_kind));
      const primary = layered[layered.length - 1];
      const title = primary.label || primary.asset_id || primary.entity_kind || 'entity';
      const delay = (((primary.x || 0) * 5 + (primary.y || 0) * 3) % 11) / 10;
      const token = entityToken(primary.asset_id, primary.entity_kind);
      const badge = cellEntities.length > 1
        ? `<span class="tilecrowd">+${cellEntities.length - 1}</span>`
        : '';
      return `<span class="tileentity ${escapeHtml(primary.entity_kind || 'entity')}" style="--entity-dx:0px; --entity-dy:0px; --entity-delay:-${delay.toFixed(1)}s" title="${escapeHtml(title)}">${escapeHtml(token)}</span>${badge}`;
    }

    function tileCell(tileId, cellEntities) {
      const hasEntity = Array.isArray(cellEntities) && cellEntities.length ? ' has-entity' : '';
      const tileClass = tileId === 'tile_floor_clean' ? ' floor' : '';
      const overlay = renderEntityOverlay(cellEntities);
      const token = tileToken(tileId);
      const glyph = token.trim()
        ? `<span class="tileglyph glyph-outline">${escapeHtml(token)}</span>`
        : '';
      return `<div class="tilecell${tileClass}${hasEntity}" title="${escapeHtml(tileId || 'unknown')}">${glyph}${overlay}</div>`;
    }

    function renderTilemap(tilemap, floorPlacement, runtimeAssets, assignedAgentCount) {
      if (!tilemap || !tilemap.width || !tilemap.height || !Array.isArray(tilemap.tiles)) {
        return '<div class="metric">tilemap: unavailable</div>';
      }
      const width = tilemap.width;
      const height = tilemap.height;
      const wallIds = [...new Set(
        tilemap.tiles.filter(v => typeof v === 'string' && v.startsWith('tile_wall_'))
      )].sort();
      const sample = wallIds.slice(0, 5).join(', ');
      const byCell = buildPlacementCellMap(floorPlacement, width, height, runtimeAssets);
      const cells = tilemap.tiles.map((tileId, index) => {
        const x = index % width;
        const y = Math.floor(index / width);
        return tileCell(tileId, byCell[cellKey(x, y)] || []);
      }).join('');
      const placedCount = Object.values(byCell).reduce((sum, cell) => sum + (Array.isArray(cell) ? cell.length : 0), 0);
      const crowdedCells = Object.values(byCell).filter((cell) => Array.isArray(cell) && cell.length > 1).length;
      const propCount = floorPlacement && Array.isArray(floorPlacement.props) ? floorPlacement.props.length : 0;
      const agentCount = floorPlacement && Array.isArray(floorPlacement.agents) ? floorPlacement.agents.length : 0;
      const fxCount = floorPlacement && Array.isArray(floorPlacement.fx) ? floorPlacement.fx.length : 0;
      const assigned = Number.isInteger(assignedAgentCount) ? assignedAgentCount : agentCount;
      const hiddenAgents = Math.max(0, assigned - agentCount);
      return `<div class="metric">tilemap: ${width}x${tilemap.height} | wall ids: ${wallIds.length} | render: ascii_v1</div>
              <div class="metric">runtime placed entities: ${placedCount} | props: ${propCount} | agents(rendered): ${agentCount} | agents(hidden): ${hiddenAgents} | fx: ${fxCount} | crowded cells: ${crowdedCells}</div>
              <div class="metric">layout policy: tile=3ft | min aisle=1 tile | main corridor=2 tiles | turning pocket=2x2</div>
              <div class="metric">walls: ${escapeHtml(sample || 'none')}</div>
              <div class="tilegrid" style="grid-template-columns: repeat(${width}, 24px);">${cells}</div>`;
    }

    function floorPlacementFor(runtimeFloorPlacements, floorId) {
      const floors = runtimeFloorPlacements && Array.isArray(runtimeFloorPlacements.floors)
        ? runtimeFloorPlacements.floors
        : [];
      return floors.find((f) => f && f.floor_id === floorId) || null;
    }

    function mergeRuntimeFloorPlacementDelta(basePlacements, deltaPlacements) {
      const baseFloors = basePlacements && Array.isArray(basePlacements.floors)
        ? basePlacements.floors
        : [];
      const deltaFloors = deltaPlacements && Array.isArray(deltaPlacements.floors)
        ? deltaPlacements.floors
        : [];
      const removedFloorIds = deltaPlacements && Array.isArray(deltaPlacements.removed_floor_ids)
        ? deltaPlacements.removed_floor_ids
        : [];
      const byId = new Map();
      baseFloors.forEach((floor) => {
        if (floor && typeof floor.floor_id === 'string') {
          byId.set(floor.floor_id, floor);
        }
      });
      removedFloorIds.forEach((id) => {
        if (typeof id === 'string') {
          byId.delete(id);
        }
      });
      const applyEntityPatch = (baseEntries, upserts, removeIds) => {
        const map = new Map();
        const base = Array.isArray(baseEntries) ? baseEntries : [];
        base.forEach((entry) => {
          if (entry && typeof entry.entity_id === 'string') {
            map.set(entry.entity_id, entry);
          }
        });
        const removals = Array.isArray(removeIds) ? removeIds : [];
        removals.forEach((id) => {
          if (typeof id === 'string') map.delete(id);
        });
        const adds = Array.isArray(upserts) ? upserts : [];
        adds.forEach((entry) => {
          if (entry && typeof entry.entity_id === 'string') {
            map.set(entry.entity_id, entry);
          }
        });
        return Array.from(map.values());
      };
      deltaFloors.forEach((delta) => {
        if (!delta || typeof delta.floor_id !== 'string') return;
        const prior = byId.get(delta.floor_id) || {
          floor_id: delta.floor_id,
          floor_name: delta.floor_name || delta.floor_id,
          width: Number(delta.width) || 1,
          height: Number(delta.height) || 1,
          props: [],
          agents: [],
          fx: [],
        };
        const next = {
          floor_id: delta.floor_id,
          floor_name: typeof delta.floor_name === 'string' ? delta.floor_name : prior.floor_name,
          width: Number(delta.width) || prior.width || 1,
          height: Number(delta.height) || prior.height || 1,
          props: applyEntityPatch(prior.props, delta.upsert_props, delta.remove_prop_ids),
          agents: applyEntityPatch(prior.agents, delta.upsert_agents, delta.remove_agent_ids),
          fx: applyEntityPatch(prior.fx, delta.upsert_fx, delta.remove_fx_ids),
        };
        byId.set(delta.floor_id, next);
      });
      return { floors: Array.from(byId.values()) };
    }

    function renderFloors(payload) {
      floorsEl.innerHTML = '';
      payload.floors.forEach(f => {
        const placement = floorPlacementFor(payload.runtime_floor_placements, f.id);
        const assignedAgentCount = Array.isArray(payload.agents)
          ? payload.agents.filter((a) => a && a.location_floor_id === f.id).length
          : 0;
        const div = document.createElement('div');
        div.className = 'floor';
        div.innerHTML = `<div class="row"><strong>${f.name}</strong><span>${f.floor_type}</span></div>
                         <div class="metric">cap: ${f.capacity} | occ: ${f.occupancy} | rent: ${f.rent_curve}</div>
                         ${renderTilemap(f.tilemap, placement, payload.runtime_assets, assignedAgentCount)}`;
        floorsEl.appendChild(div);
      });
    }

    function renderSnapshot(payload) {
      latestSnapshotPayload = payload;
      tickEl.textContent = `tick: ${payload.tick}`;
      renderFloors(payload);
      economyEl.textContent = `wages: ${payload.economy.wages_paid}, rent: ${payload.economy.rent_collected}, service: ${payload.economy.service_spend}, penalties: ${payload.economy.unmet_need_penalty}`;
      alertsEl.innerHTML = '';
      payload.alerts.forEach(a => {
        const div = document.createElement('div');
        div.className = 'alert';
        div.textContent = `[${a.category}] ${a.message}`;
        alertsEl.appendChild(div);
      });
      renderRuntimeAssets(payload.runtime_assets);
    }

    function renderAlerts() {
      alertsEl.innerHTML = alerts.slice(-12).map(a => `<div class="alert">[${a.category}] ${a.message}</div>`).join('');
    }

    function renderAssetSamples() {
      const sampleIds = [
        'agent_coder_idle',
        'prop_push_desk',
        'portrait_coding_neutral',
        'ui_state_healthy',
        'fx_upload_burst',
        'tile_wall_corner_top_left',
        'tile_floor_clean',
        'openai_subscription_probe',
      ];
      assetSamplesEl.innerHTML = sampleIds.map((assetId) => {
        const token = entityToken(assetId, inferEntityKind(assetId));
        return `<div class="sample-card"><div class="sample-art glyph-outline">${escapeHtml(token)}</div><div class="sample-id">${escapeHtml(assetId)}</div></div>`;
      }).join('');
    }

    function renderRuntimeBinding(binding) {
      const assetId = binding && typeof binding.asset_id === 'string' ? binding.asset_id : '';
      const label = binding && typeof binding.label === 'string' ? binding.label : '';
      const entityId = binding && typeof binding.entity_id === 'string' ? binding.entity_id : '';
      const token = entityToken(assetId, inferEntityKind(assetId));
      const art = `<div class="sample-art glyph-outline">${escapeHtml(token)}</div>`;
      return `<div class="sample-card">${art}<div><div class="sample-id">${escapeHtml(assetId || 'unknown')}</div><div class="runtime-meta">${escapeHtml(label || entityId || 'entity')}</div></div></div>`;
    }

    function renderRuntimeGroup(title, bindings) {
      if (!Array.isArray(bindings) || bindings.length === 0) {
        return `<div class="runtime-group"><div class="runtime-head">${escapeHtml(title)}: none</div></div>`;
      }
      const shown = bindings.slice(0, 8);
      return `<div class="runtime-group"><div class="runtime-head">${escapeHtml(title)}: ${bindings.length}</div><div class="sample-grid">${shown.map(renderRuntimeBinding).join('')}</div></div>`;
    }

    function renderRuntimeAssets(runtimeAssets) {
      if (!runtimeAssetsEl) return;
      if (!runtimeAssets || typeof runtimeAssets !== 'object') {
        runtimeAssetsEl.innerHTML = '<div class="metric">runtime assets: unavailable</div>';
        return;
      }
      runtimeAssetsEl.innerHTML = [
        renderRuntimeGroup('Agents', runtimeAssets.agents),
        renderRuntimeGroup('Props', runtimeAssets.props),
        renderRuntimeGroup('UI', runtimeAssets.ui),
        renderRuntimeGroup('FX', runtimeAssets.fx),
      ].join('');
    }

    function mergeRuntimeAssetsDelta(baseAssets, deltaAssets) {
      const base = baseAssets && typeof baseAssets === 'object' ? baseAssets : {};
      const delta = deltaAssets && typeof deltaAssets === 'object' ? deltaAssets : {};
      const applyAssetPatch = (baseEntries, upserts, removeIds) => {
        const map = new Map();
        const baseList = Array.isArray(baseEntries) ? baseEntries : [];
        baseList.forEach((entry) => {
          if (entry && typeof entry.entity_id === 'string') {
            map.set(entry.entity_id, entry);
          }
        });
        const removals = Array.isArray(removeIds) ? removeIds : [];
        removals.forEach((id) => {
          if (typeof id === 'string') {
            map.delete(id);
          }
        });
        const adds = Array.isArray(upserts) ? upserts : [];
        adds.forEach((entry) => {
          if (entry && typeof entry.entity_id === 'string') {
            map.set(entry.entity_id, entry);
          }
        });
        return Array.from(map.values());
      };
      return {
        agents: applyAssetPatch(base.agents, delta.upsert_agents, delta.remove_agent_ids),
        props: applyAssetPatch(base.props, delta.upsert_props, delta.remove_prop_ids),
        ui: applyAssetPatch(base.ui, delta.upsert_ui, delta.remove_ui_ids),
        fx: applyAssetPatch(base.fx, delta.upsert_fx, delta.remove_fx_ids),
      };
    }

    async function loadTileCatalog() {
      assetStatusEl.textContent = 'render mode: ascii_v1 (asset images bypassed)';
      renderAssetSamples();
    }

    async function boot() {
      await loadTileCatalog();
      const bootResp = await fetch('/tower/tower-1/bootstrap');
      const bootData = await bootResp.json();
      renderSnapshot(bootData.snapshot.payload);

      const wsProto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${wsProto}//${location.host}${bootData.ws_path}`);
      ws.binaryType = 'arraybuffer';
      ws.onmessage = async (ev) => {
        let raw = '';
        if (typeof ev.data === 'string') {
          raw = ev.data;
        } else if (ev.data instanceof ArrayBuffer) {
          raw = new TextDecoder().decode(ev.data);
        } else if (ev.data && typeof ev.data.text === 'function') {
          raw = await ev.data.text();
        } else {
          return;
        }
        let msg = null;
        try {
          msg = JSON.parse(raw);
        } catch (_) {
          return;
        }
        if (msg.type === 'WorldDiff') {
          tickEl.textContent = `tick: ${msg.payload.tick}`;
          economyEl.textContent = `tower health: ${msg.payload.metrics_delta.tower_health.toFixed(1)} | alerts: ${msg.payload.metrics_delta.alert_count}`;
          const mergedRuntimeAssets = mergeRuntimeAssetsDelta(
            latestSnapshotPayload && typeof latestSnapshotPayload === 'object'
              ? latestSnapshotPayload.runtime_assets
              : null,
            msg.payload.runtime_assets_delta
          );
          if (latestSnapshotPayload && typeof latestSnapshotPayload === 'object') {
            latestSnapshotPayload.tick = msg.payload.tick;
            latestSnapshotPayload.runtime_assets = mergedRuntimeAssets;
            latestSnapshotPayload.runtime_floor_placements = mergeRuntimeFloorPlacementDelta(
              latestSnapshotPayload.runtime_floor_placements,
              msg.payload.runtime_floor_placements_delta
            );
            renderFloors(latestSnapshotPayload);
          }
          renderRuntimeAssets(mergedRuntimeAssets);
        } else if (msg.type === 'AlertRaised') {
          alerts.push(msg.payload);
          renderAlerts();
        } else if (msg.type === 'WorldSnapshot') {
          renderSnapshot(msg.payload);
        }
      };
    }

    boot().catch(err => {
      alertsEl.innerHTML = `<div class="alert">bootstrap error: ${err}</div>`;
    });
  </script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_mapping_uses_worker_alert_for_stressed_worker() {
        let mut world = initialize_tower(42);
        let first = world.agents.first_mut().expect("agent exists");
        first.role = "worker".to_string();
        first.mood = 20;
        first.needs.energy = 10;
        first.current_goal = AgentGoal::Work;

        assert_eq!(map_agent_asset_id(first), "agent_worker_alert");
    }

    #[test]
    fn runtime_asset_mappings_populate_all_entity_groups() {
        let world = initialize_tower(1337);
        let mappings = build_runtime_asset_mappings(&world);

        assert_eq!(mappings.agents.len(), world.agents.len());
        assert_eq!(mappings.props.len(), world.floors.len());
        assert!(mappings.ui.len() >= 3);
        assert!(mappings.fx.len() >= world.floors.len());
    }

    #[test]
    fn world_snapshot_includes_runtime_assets() {
        let world = initialize_tower(7);
        let snapshot = build_world_snapshot(&world);
        match snapshot {
            StreamPayload::WorldSnapshot {
                runtime_assets,
                runtime_floor_placements,
                ..
            } => {
                assert!(!runtime_assets.agents.is_empty());
                assert!(!runtime_assets.props.is_empty());
                assert!(!runtime_assets.ui.is_empty());
                assert!(!runtime_assets.fx.is_empty());
                assert_eq!(runtime_floor_placements.floors.len(), world.floors.len());
            }
            _ => panic!("expected world snapshot"),
        }
    }

    #[test]
    fn world_diff_runtime_assets_delta_populates_per_tick() {
        let mut world = initialize_tower(9);
        let mut tick = run_tick(&mut world);
        assert!(tick.diff.runtime_assets_delta.upsert_agents.is_empty());
        assert!(tick.diff.runtime_assets_delta.upsert_props.is_empty());

        let full_assets = build_runtime_asset_mappings(&world);
        tick.diff.runtime_assets_delta = runtime_assets_delta_for_diff(None, &full_assets);
        assert!(!tick.diff.runtime_assets_delta.upsert_agents.is_empty());
        assert!(!tick.diff.runtime_assets_delta.upsert_props.is_empty());
        assert!(!tick.diff.runtime_assets_delta.upsert_ui.is_empty());
        assert!(!tick.diff.runtime_assets_delta.upsert_fx.is_empty());
        assert!(tick.diff.runtime_assets_delta.remove_agent_ids.is_empty());

        let full_placements = build_runtime_floor_placements(&world);
        tick.diff.runtime_floor_placements_delta =
            runtime_floor_placements_delta_for_diff(None, &full_placements);
        assert_eq!(
            tick.diff.runtime_floor_placements_delta.floors.len(),
            world.floors.len()
        );
        assert!(
            tick.diff
                .runtime_floor_placements_delta
                .floors
                .iter()
                .all(|f| !f.upsert_props.is_empty())
        );
    }

    #[test]
    fn runtime_assets_delta_is_empty_when_unchanged() {
        let world = initialize_tower(27);
        let current = build_runtime_asset_mappings(&world);
        let delta = runtime_assets_delta_for_diff(Some(&current), &current);
        assert!(delta.upsert_agents.is_empty());
        assert!(delta.upsert_props.is_empty());
        assert!(delta.upsert_ui.is_empty());
        assert!(delta.upsert_fx.is_empty());
        assert!(delta.remove_agent_ids.is_empty());
        assert!(delta.remove_prop_ids.is_empty());
        assert!(delta.remove_ui_ids.is_empty());
        assert!(delta.remove_fx_ids.is_empty());
    }

    #[test]
    fn runtime_assets_delta_reports_only_changed_bindings() {
        let world = initialize_tower(29);
        let mut current = build_runtime_asset_mappings(&world);
        let baseline = current.clone();
        current.agents[0].asset_id = "agent_worker_alert".to_string();

        let delta = runtime_assets_delta_for_diff(Some(&baseline), &current);
        assert_eq!(delta.upsert_agents.len(), 1);
        assert!(delta.upsert_props.is_empty());
        assert!(delta.upsert_ui.is_empty());
        assert!(delta.upsert_fx.is_empty());
        assert!(delta.remove_agent_ids.is_empty());
    }

    #[test]
    fn runtime_assets_delta_ignores_label_only_changes() {
        let world = initialize_tower(33);
        let mut current = build_runtime_asset_mappings(&world);
        let baseline = current.clone();
        current.agents[0].label = "worker relabel".to_string();

        let delta = runtime_assets_delta_for_diff(Some(&baseline), &current);
        assert!(delta.upsert_agents.is_empty());
        assert!(delta.remove_agent_ids.is_empty());
    }

    #[test]
    fn agent_asset_hysteresis_can_hold_previous_visual() {
        let agent = sim::Agent {
            id: Uuid::nil(),
            role: "worker".to_string(),
            credits: 0,
            needs: sim::Needs {
                hunger: 90,
                energy: 90,
                social: 80,
                comfort: 80,
            },
            mood: 80,
            current_goal: AgentGoal::Rest,
            location_floor_id: Uuid::nil(),
            lease_id: None,
            debt_ticks: 0,
        };
        let raw = map_agent_asset_id(&agent);
        assert_eq!(raw, "agent_worker_idle");
        let sticky = map_agent_asset_id_with_hysteresis(&agent, raw, Some("agent_worker_move"), 1);
        assert_eq!(sticky, "agent_worker_move");
    }

    #[test]
    fn agent_asset_hysteresis_escalates_immediately_on_severe_state() {
        let agent = sim::Agent {
            id: Uuid::nil(),
            role: "service".to_string(),
            credits: 0,
            needs: sim::Needs {
                hunger: 5,
                energy: 7,
                social: 80,
                comfort: 80,
            },
            mood: 15,
            current_goal: AgentGoal::Rest,
            location_floor_id: Uuid::nil(),
            lease_id: None,
            debt_ticks: 0,
        };
        let raw = map_agent_asset_id(&agent);
        assert_eq!(raw, "agent_coder_fail");
        let sticky = map_agent_asset_id_with_hysteresis(&agent, raw, Some("agent_coder_idle"), 2);
        assert_eq!(sticky, "agent_coder_fail");
    }

    #[test]
    fn agent_floor_transition_hysteresis_holds_when_not_due() {
        let agent = sim::Agent {
            id: Uuid::nil(),
            role: "worker".to_string(),
            credits: 0,
            needs: sim::Needs {
                hunger: 80,
                energy: 80,
                social: 80,
                comfort: 80,
            },
            mood: 70,
            current_goal: AgentGoal::Work,
            location_floor_id: Uuid::nil(),
            lease_id: None,
            debt_ticks: 0,
        };
        assert!(!should_transition_agent_floor(&agent, 1));
    }

    #[test]
    fn agent_floor_transition_hysteresis_allows_relocate_or_severe() {
        let relocate_agent = sim::Agent {
            id: Uuid::nil(),
            role: "worker".to_string(),
            credits: 0,
            needs: sim::Needs {
                hunger: 80,
                energy: 80,
                social: 80,
                comfort: 80,
            },
            mood: 70,
            current_goal: AgentGoal::Relocate,
            location_floor_id: Uuid::nil(),
            lease_id: None,
            debt_ticks: 0,
        };
        assert!(should_transition_agent_floor(&relocate_agent, 1));

        let severe_agent = sim::Agent {
            current_goal: AgentGoal::Work,
            mood: 10,
            needs: sim::Needs {
                hunger: 5,
                energy: 6,
                social: 80,
                comfort: 80,
            },
            ..relocate_agent
        };
        assert!(should_transition_agent_floor(&severe_agent, 1));
    }

    #[test]
    fn runtime_floor_placements_stay_in_bounds_and_off_walls() {
        let world = initialize_tower(21);
        let placements = build_runtime_floor_placements(&world);

        for floor_placement in &placements.floors {
            let floor = world
                .floors
                .iter()
                .find(|f| f.id.to_string() == floor_placement.floor_id)
                .expect("floor exists");
            let all = floor_placement
                .props
                .iter()
                .chain(floor_placement.agents.iter())
                .chain(floor_placement.fx.iter());
            for placed in all {
                assert!(placed.x < floor_placement.width);
                assert!(placed.y < floor_placement.height);
                let tile = floor_tile_at(&floor.tilemap, placed.x, placed.y)
                    .expect("placement tile exists in tilemap");
                assert!(
                    !tile.starts_with("tile_wall_"),
                    "placement landed on wall tile: floor={} asset={} at ({},{}) tile={}",
                    floor_placement.floor_name,
                    placed.asset_id,
                    placed.x,
                    placed.y,
                    tile
                );
            }
        }
    }

    #[test]
    fn runtime_floor_props_preserve_circulation_clearance() {
        let world = initialize_tower(23);
        let placements = build_runtime_floor_placements(&world);

        for floor_placement in &placements.floors {
            let floor = world
                .floors
                .iter()
                .find(|f| f.id.to_string() == floor_placement.floor_id)
                .expect("floor exists");
            let walkable = floor_walkable_cells(&floor.tilemap);
            let reserved = reserved_circulation_cells(&floor.tilemap, &walkable);
            for prop in &floor_placement.props {
                assert!(
                    cell_clear_of_reserved(
                        (prop.x, prop.y),
                        &reserved,
                        OFFICE_MIN_AISLE_WIDTH_TILES
                    ),
                    "prop entered circulation buffer: floor={} asset={} at ({},{})",
                    floor_placement.floor_name,
                    prop.asset_id,
                    prop.x,
                    prop.y
                );
            }
        }
    }

    #[test]
    fn runtime_floor_agent_visibility_is_density_capped() {
        let world = initialize_tower(25);
        let placements = build_runtime_floor_placements(&world);

        for floor in &world.floors {
            let assigned_agents = world
                .agents
                .iter()
                .filter(|agent| agent.location_floor_id == floor.id)
                .count();
            let walkable = floor_walkable_cells(&floor.tilemap);
            let cap = max_visible_agents_for_floor(floor.floor_type, walkable.len());
            let placement = placements
                .floors
                .iter()
                .find(|f| f.floor_id == floor.id.to_string())
                .expect("floor placement exists");
            assert!(
                placement.agents.len() <= cap,
                "rendered agents exceeded cap: floor={} rendered={} cap={}",
                floor.name,
                placement.agents.len(),
                cap
            );
            assert!(
                placement.agents.len() <= assigned_agents,
                "rendered agents exceeded assigned agents: floor={} rendered={} assigned={}",
                floor.name,
                placement.agents.len(),
                assigned_agents
            );
        }
    }

    #[test]
    fn runtime_floor_placement_delta_reports_only_changed_floors() {
        let world = initialize_tower(31);
        let mut current = build_runtime_floor_placements(&world);
        let baseline = current.clone();

        let initial_delta = runtime_floor_placements_delta_for_diff(None, &current);
        assert_eq!(initial_delta.floors.len(), current.floors.len());
        assert!(initial_delta.removed_floor_ids.is_empty());

        let target_id = current
            .floors
            .first()
            .expect("placement floor exists")
            .floor_id
            .clone();
        current.floors[0].props[0].asset_id = "prop_pull_desk".to_string();

        let delta = runtime_floor_placements_delta_for_diff(Some(&baseline), &current);
        assert_eq!(delta.floors.len(), 1);
        assert_eq!(delta.floors[0].floor_id, target_id);
        assert_eq!(delta.floors[0].upsert_props.len(), 1);
        assert!(delta.floors[0].upsert_agents.is_empty());
        assert!(delta.floors[0].upsert_fx.is_empty());
        assert!(delta.floors[0].remove_prop_ids.is_empty());
    }

    #[test]
    fn runtime_floor_placement_delta_is_empty_when_unchanged() {
        let world = initialize_tower(41);
        let current = build_runtime_floor_placements(&world);
        let delta = runtime_floor_placements_delta_for_diff(Some(&current), &current);
        assert!(delta.floors.is_empty());
        assert!(delta.removed_floor_ids.is_empty());
    }

    #[test]
    fn runtime_floor_placement_delta_ignores_label_only_changes() {
        let world = initialize_tower(43);
        let mut current = build_runtime_floor_placements(&world);
        let baseline = current.clone();
        let floor = current
            .floors
            .iter_mut()
            .find(|f| !f.agents.is_empty())
            .expect("floor with agent exists");
        floor.agents[0].label = "worker relabel".to_string();

        let delta = runtime_floor_placements_delta_for_diff(Some(&baseline), &current);
        assert!(delta.floors.is_empty());
        assert!(delta.removed_floor_ids.is_empty());
    }

    #[test]
    fn runtime_floor_placement_delta_ignores_agent_asset_only_changes() {
        let world = initialize_tower(53);
        let mut current = build_runtime_floor_placements(&world);
        let baseline = current.clone();
        let floor = current
            .floors
            .iter_mut()
            .find(|f| !f.agents.is_empty())
            .expect("floor with agent exists");
        floor.agents[0].asset_id = "agent_worker_alert".to_string();

        let delta = runtime_floor_placements_delta_for_diff(Some(&baseline), &current);
        assert!(delta.floors.is_empty());
        assert!(delta.removed_floor_ids.is_empty());
    }

    #[test]
    fn optional_floor_transition_cap_limits_non_urgent_moves() {
        let mut world = initialize_tower(57);
        let previous_floor_id = world.floors[0].id;
        let desired_floor_id = world.floors[1].id;
        for agent in &mut world.agents {
            agent.location_floor_id = desired_floor_id;
            agent.current_goal = AgentGoal::Work;
            agent.mood = 70;
            agent.needs.hunger = 70;
            agent.needs.energy = 70;
            agent.needs.social = 70;
            agent.needs.comfort = 70;
        }
        let tick = (0..400u64)
            .find(|tick| {
                world
                    .agents
                    .iter()
                    .filter(|agent| should_allow_optional_floor_transition(agent, *tick))
                    .count()
                    > AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TICK
            })
            .expect("expected at least one tick with optional transitions above cap");
        world.tick = tick;

        let previous = RuntimeFloorPlacements {
            floors: world
                .floors
                .iter()
                .map(|floor| RuntimeFloorPlacement {
                    floor_id: floor.id.to_string(),
                    floor_name: floor.name.clone(),
                    width: floor.tilemap.width.max(1),
                    height: floor.tilemap.height.max(1),
                    props: vec![],
                    agents: if floor.id == previous_floor_id {
                        world
                            .agents
                            .iter()
                            .map(|agent| RuntimePlacedAsset {
                                entity_id: agent.id.to_string(),
                                entity_kind: "agent".to_string(),
                                label: "prev".to_string(),
                                asset_id: "agent_worker_idle".to_string(),
                                x: 0,
                                y: 0,
                            })
                            .collect()
                    } else {
                        vec![]
                    },
                    fx: vec![],
                })
                .collect(),
        };

        let current = build_runtime_floor_placements_with_previous(&world, Some(&previous));
        let moved_to_desired = current
            .floors
            .iter()
            .find(|floor| floor.floor_id == desired_floor_id.to_string())
            .map(|floor| floor.agents.len())
            .unwrap_or_default();
        let expected_cap = AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TICK
            .min(AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK);
        assert!(
            moved_to_desired <= expected_cap,
            "moved_to_desired={} cap={}",
            moved_to_desired,
            expected_cap
        );
    }

    #[test]
    fn optional_floor_transition_cap_limits_per_target_floor() {
        let mut world = initialize_tower(59);
        let previous_floor_id = world.floors[0].id;
        let desired_floor_a = world.floors[1].id;
        let desired_floor_b = world.floors[2].id;
        for (idx, agent) in world.agents.iter_mut().enumerate() {
            agent.location_floor_id = if idx % 2 == 0 {
                desired_floor_a
            } else {
                desired_floor_b
            };
            agent.current_goal = AgentGoal::Work;
            agent.mood = 70;
            agent.needs.hunger = 70;
            agent.needs.energy = 70;
            agent.needs.social = 70;
            agent.needs.comfort = 70;
        }
        let tick = (0..400u64)
            .find(|tick| {
                let mut by_target: HashMap<Uuid, usize> = HashMap::new();
                for agent in &world.agents {
                    if should_allow_optional_floor_transition(agent, *tick) {
                        *by_target.entry(agent.location_floor_id).or_default() += 1;
                    }
                }
                by_target
                    .values()
                    .copied()
                    .max()
                    .is_some_and(|max_for_floor| {
                        max_for_floor
                            > AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK
                    })
            })
            .expect("expected at least one tick above per-floor cap");
        world.tick = tick;

        let previous = RuntimeFloorPlacements {
            floors: world
                .floors
                .iter()
                .map(|floor| RuntimeFloorPlacement {
                    floor_id: floor.id.to_string(),
                    floor_name: floor.name.clone(),
                    width: floor.tilemap.width.max(1),
                    height: floor.tilemap.height.max(1),
                    props: vec![],
                    agents: if floor.id == previous_floor_id {
                        world
                            .agents
                            .iter()
                            .map(|agent| RuntimePlacedAsset {
                                entity_id: agent.id.to_string(),
                                entity_kind: "agent".to_string(),
                                label: "prev".to_string(),
                                asset_id: "agent_worker_idle".to_string(),
                                x: 0,
                                y: 0,
                            })
                            .collect()
                    } else {
                        vec![]
                    },
                    fx: vec![],
                })
                .collect(),
        };

        let current = build_runtime_floor_placements_with_previous(&world, Some(&previous));
        let moved_a = current
            .floors
            .iter()
            .find(|floor| floor.floor_id == desired_floor_a.to_string())
            .map(|floor| floor.agents.len())
            .unwrap_or_default();
        let moved_b = current
            .floors
            .iter()
            .find(|floor| floor.floor_id == desired_floor_b.to_string())
            .map(|floor| floor.agents.len())
            .unwrap_or_default();
        assert!(
            moved_a <= AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK,
            "moved_a={} cap={}",
            moved_a,
            AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK
        );
        assert!(
            moved_b <= AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK,
            "moved_b={} cap={}",
            moved_b,
            AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TARGET_FLOOR_PER_TICK
        );
        assert!(
            moved_a + moved_b <= AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TICK,
            "moved_total={} cap={}",
            moved_a + moved_b,
            AGENT_OPTIONAL_FLOOR_TRANSITION_MAX_PER_TICK
        );
    }
}
