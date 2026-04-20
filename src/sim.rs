use chrono::{DateTime, Utc};
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub const MAX_AGENTS: usize = 100;
pub const DEFAULT_TOWER_ID: &str = "tower-1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FloorType {
    Residential,
    Commercial,
    Service,
    Utility,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FloorTileMap {
    pub width: u8,
    pub height: u8,
    pub tiles: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentGoal {
    Work,
    Eat,
    Rest,
    Socialize,
    Relocate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LeaseStatus {
    Active,
    Delinquent,
    Evicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Needs {
    pub hunger: u8,
    pub energy: u8,
    pub social: u8,
    pub comfort: u8,
}

impl Needs {
    pub fn avg_satisfaction(&self) -> f32 {
        (self.hunger as f32 + self.energy as f32 + self.social as f32 + self.comfort as f32) / 4.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub role: String,
    pub credits: i32,
    pub needs: Needs,
    pub mood: i32,
    pub current_goal: AgentGoal,
    pub location_floor_id: Uuid,
    pub lease_id: Option<Uuid>,
    pub debt_ticks: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Floor {
    pub id: Uuid,
    pub name: String,
    pub floor_type: FloorType,
    pub capacity: u16,
    pub occupancy: u16,
    pub upkeep: i32,
    pub rent_curve: i32,
    #[serde(default)]
    pub tilemap: FloorTileMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub unit_floor_id: Uuid,
    pub rent: i32,
    pub due_tick: u64,
    pub status: LeaseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomyTick {
    pub wages_paid: i64,
    pub rent_collected: i64,
    pub service_spend: i64,
    pub unmet_need_penalty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TowerState {
    pub tower_id: String,
    pub tick: u64,
    pub started_at: DateTime<Utc>,
    pub agents: Vec<Agent>,
    pub floors: Vec<Floor>,
    pub leases: HashMap<Uuid, Lease>,
    pub economy: EconomyTick,
    pub alerts: Vec<Alert>,
    pub policies: Vec<Policy>,
    pub active_signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub severity: String,
    pub category: String,
    pub message: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrace {
    pub decision: AgentGoal,
    pub inputs: HashMap<String, i32>,
    pub priority_scores: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeAssetBinding {
    pub entity_id: String,
    pub entity_kind: String,
    pub label: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeAssetMappings {
    #[serde(default)]
    pub agents: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub props: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub ui: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub fx: Vec<RuntimeAssetBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RuntimeAssetMappingDeltas {
    #[serde(default)]
    pub upsert_agents: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub upsert_props: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub upsert_ui: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub upsert_fx: Vec<RuntimeAssetBinding>,
    #[serde(default)]
    pub remove_agent_ids: Vec<String>,
    #[serde(default)]
    pub remove_prop_ids: Vec<String>,
    #[serde(default)]
    pub remove_ui_ids: Vec<String>,
    #[serde(default)]
    pub remove_fx_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePlacedAsset {
    pub entity_id: String,
    pub entity_kind: String,
    pub label: String,
    pub asset_id: String,
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeFloorPlacement {
    pub floor_id: String,
    pub floor_name: String,
    pub width: u8,
    pub height: u8,
    #[serde(default)]
    pub props: Vec<RuntimePlacedAsset>,
    #[serde(default)]
    pub agents: Vec<RuntimePlacedAsset>,
    #[serde(default)]
    pub fx: Vec<RuntimePlacedAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RuntimeFloorPlacements {
    #[serde(default)]
    pub floors: Vec<RuntimeFloorPlacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeFloorPlacementDelta {
    pub floor_id: String,
    pub floor_name: String,
    pub width: u8,
    pub height: u8,
    #[serde(default)]
    pub upsert_props: Vec<RuntimePlacedAsset>,
    #[serde(default)]
    pub upsert_agents: Vec<RuntimePlacedAsset>,
    #[serde(default)]
    pub upsert_fx: Vec<RuntimePlacedAsset>,
    #[serde(default)]
    pub remove_prop_ids: Vec<String>,
    #[serde(default)]
    pub remove_agent_ids: Vec<String>,
    #[serde(default)]
    pub remove_fx_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RuntimeFloorPlacementDeltas {
    #[serde(default)]
    pub floors: Vec<RuntimeFloorPlacementDelta>,
    #[serde(default)]
    pub removed_floor_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimEvent {
    pub id: Uuid,
    pub tick: u64,
    pub timestamp: DateTime<Utc>,
    pub category: String,
    pub severity: String,
    pub message: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsPoint {
    pub tick: u64,
    pub timestamp: DateTime<Utc>,
    pub occupancy: f32,
    pub avg_need_satisfaction: f32,
    pub liquidity: f32,
    pub alert_count: usize,
    pub tower_health: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDiff {
    pub tick: u64,
    pub changed_agents: Vec<Uuid>,
    pub changed_floors: Vec<Uuid>,
    pub metrics_delta: MetricsPoint,
    pub new_events: Vec<SimEvent>,
    #[serde(default)]
    pub runtime_assets_delta: RuntimeAssetMappingDeltas,
    #[serde(default)]
    pub runtime_floor_placements_delta: RuntimeFloorPlacementDeltas,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepartmentHealth {
    Healthy,
    Degraded,
    Stalled,
    Broken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeskActivityUpdate {
    pub tower_id: String,
    pub floor_id: String,
    pub department_id: String,
    pub desk_id: String,
    pub event_type: String,
    pub task_id: Option<String>,
    pub agent_id: Option<String>,
    pub active_until: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentStateUpdate {
    pub tower_id: String,
    pub floor_id: String,
    pub department_id: String,
    pub state: DepartmentHealth,
    pub reason: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrestige {
    pub tower_id: String,
    pub floor_id: String,
    pub prestige: f32,
    pub successful_runs: u64,
    pub active_runtime_seconds: u64,
    pub current_streak_days: u32,
    pub last_active_at: DateTime<Utc>,
    pub decay_window_hours: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingEntry {
    pub rank: usize,
    pub tower_id: String,
    pub floor_id: String,
    pub prestige: f32,
    pub current_streak_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub key: String,
    pub value: f32,
    pub created_at_tick: u64,
    pub expires_at_tick: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub signal_type: String,
    pub target_zone: String,
    pub intensity: f32,
    pub created_at_tick: u64,
    pub expires_at_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum StreamPayload {
    ServerHello {
        server_time: DateTime<Utc>,
        tick_rate: u64,
        tower_id: String,
    },
    WorldSnapshot {
        tick: u64,
        tower: String,
        floors: Vec<Floor>,
        agents: Vec<Agent>,
        economy: EconomyTick,
        alerts: Vec<Alert>,
        #[serde(default)]
        runtime_assets: RuntimeAssetMappings,
        #[serde(default)]
        runtime_floor_placements: RuntimeFloorPlacements,
    },
    WorldDiff(WorldDiff),
    AgentExplain {
        agent_id: Uuid,
        trace: DecisionTrace,
    },
    AlertRaised(Alert),
    SignalBroadcast(Signal),
    PolicySet(Policy),
    DeskActivity(DeskActivityUpdate),
    DepartmentState(DepartmentStateUpdate),
    PrestigeUpdated(FloorPrestige),
}

#[derive(Debug, Clone)]
pub struct TickResult {
    pub diff: WorldDiff,
    pub metrics: MetricsPoint,
    pub alerts: Vec<Alert>,
    pub decision_traces: HashMap<Uuid, DecisionTrace>,
}

const TILE_FLOOR_CLEAN: &str = "tile_floor_clean";

fn tile_index(width: u8, x: u8, y: u8) -> usize {
    y as usize * width as usize + x as usize
}

fn set_tile(map: &mut FloorTileMap, x: u8, y: u8, tile_id: &str) {
    if x >= map.width || y >= map.height {
        return;
    }
    let idx = tile_index(map.width, x, y);
    if idx < map.tiles.len() {
        map.tiles[idx] = tile_id.to_string();
    }
}

fn resolve_boundary_wall_tile(x: u8, y: u8, width: u8, height: u8) -> Option<&'static str> {
    if width < 2 || height < 2 {
        return None;
    }
    if x == 0 && y == 0 {
        return Some("tile_wall_corner_top_left");
    }
    if x == width - 1 && y == 0 {
        return Some("tile_wall_corner_top_right");
    }
    if x == 0 && y == height - 1 {
        return Some("tile_wall_corner_bottom_left");
    }
    if x == width - 1 && y == height - 1 {
        return Some("tile_wall_corner_bottom_right");
    }
    if y == 0 {
        return Some("tile_wall_edge_top");
    }
    if y == height - 1 {
        return Some("tile_wall_edge_bottom");
    }
    if x == 0 {
        return Some("tile_wall_edge_left");
    }
    if x == width - 1 {
        return Some("tile_wall_edge_right");
    }
    None
}

fn build_floor_tilemap(floor_type: FloorType) -> FloorTileMap {
    let (width, height) = match floor_type {
        FloorType::Residential => (12, 9),
        FloorType::Service => (13, 9),
        FloorType::Commercial => (14, 10),
        FloorType::Utility => (11, 8),
    };
    let mut map = FloorTileMap {
        width,
        height,
        tiles: vec![TILE_FLOOR_CLEAN.to_string(); width as usize * height as usize],
    };

    for y in 0..height {
        for x in 0..width {
            if let Some(tile_id) = resolve_boundary_wall_tile(x, y, width, height) {
                set_tile(&mut map, x, y, tile_id);
            }
        }
    }

    let door_left_x = width / 2 - 1;
    let door_right_x = width / 2;
    let door_y = height - 1;
    set_tile(&mut map, door_left_x, door_y, "tile_wall_door_left");
    set_tile(&mut map, door_right_x, door_y, "tile_wall_door_right");

    match floor_type {
        FloorType::Residential => {
            if width > 6 {
                set_tile(&mut map, 2, 0, "tile_wall_window_left");
                set_tile(&mut map, width - 3, 0, "tile_wall_window_right");
            }
        }
        FloorType::Service => {
            set_tile(&mut map, width - 1, height / 2, "tile_wall_terminal");
            set_tile(&mut map, width / 2, 0, "tile_wall_t_top");
            set_tile(&mut map, width / 2, height - 1, "tile_wall_t_bottom");
        }
        FloorType::Commercial => {
            set_tile(&mut map, width - 1, height / 2, "tile_wall_terminal");
            set_tile(&mut map, 3, 3, "tile_wall_pillar");
            set_tile(&mut map, width - 4, 3, "tile_wall_pillar_alt");
            set_tile(&mut map, width / 2, height / 2, "tile_wall_cross");
        }
        FloorType::Utility => {
            set_tile(&mut map, width - 1, height / 2, "tile_wall_terminal");
            set_tile(&mut map, width / 2, 0, "tile_wall_window_left");
        }
    }

    map
}

pub fn ensure_floor_tilemaps(state: &mut TowerState) {
    for floor in &mut state.floors {
        let expected = floor.tilemap.width as usize * floor.tilemap.height as usize;
        if floor.tilemap.width == 0
            || floor.tilemap.height == 0
            || floor.tilemap.tiles.len() != expected
        {
            floor.tilemap = build_floor_tilemap(floor.floor_type);
        }
    }
}

pub fn initialize_tower(seed: u64) -> TowerState {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut floors = vec![];

    let residential_id = Uuid::new_v4();
    let service_id = Uuid::new_v4();
    let commercial_id = Uuid::new_v4();
    floors.push(Floor {
        id: residential_id,
        name: "Residences".to_string(),
        floor_type: FloorType::Residential,
        capacity: 120,
        occupancy: MAX_AGENTS as u16,
        upkeep: 30,
        rent_curve: 18,
        tilemap: build_floor_tilemap(FloorType::Residential),
    });
    floors.push(Floor {
        id: service_id,
        name: "Food Court".to_string(),
        floor_type: FloorType::Service,
        capacity: 80,
        occupancy: 0,
        upkeep: 22,
        rent_curve: 10,
        tilemap: build_floor_tilemap(FloorType::Service),
    });
    floors.push(Floor {
        id: commercial_id,
        name: "Work Hub".to_string(),
        floor_type: FloorType::Commercial,
        capacity: 100,
        occupancy: 0,
        upkeep: 40,
        rent_curve: 12,
        tilemap: build_floor_tilemap(FloorType::Commercial),
    });

    let mut leases = HashMap::new();
    let mut agents = Vec::with_capacity(MAX_AGENTS);
    for _ in 0..MAX_AGENTS {
        let id = Uuid::new_v4();
        let lease_id = Uuid::new_v4();
        let rent = 10 + rng.random_range(0..12);
        leases.insert(
            lease_id,
            Lease {
                id: lease_id,
                agent_id: id,
                unit_floor_id: residential_id,
                rent,
                due_tick: 1,
                status: LeaseStatus::Active,
            },
        );
        agents.push(Agent {
            id,
            role: if rng.random_bool(0.65) {
                "worker".to_string()
            } else {
                "service".to_string()
            },
            credits: 200,
            needs: Needs {
                hunger: rng.random_range(55..100),
                energy: rng.random_range(55..100),
                social: rng.random_range(55..100),
                comfort: rng.random_range(55..100),
            },
            mood: 75,
            current_goal: AgentGoal::Work,
            location_floor_id: residential_id,
            lease_id: Some(lease_id),
            debt_ticks: 0,
        });
    }

    let mut state = TowerState {
        tower_id: DEFAULT_TOWER_ID.to_string(),
        tick: 0,
        started_at: Utc::now(),
        agents,
        floors,
        leases,
        economy: EconomyTick {
            wages_paid: 0,
            rent_collected: 0,
            service_spend: 0,
            unmet_need_penalty: 0,
        },
        alerts: vec![],
        policies: vec![],
        active_signals: vec![],
    };
    ensure_floor_tilemaps(&mut state);
    state
}

pub fn run_tick(state: &mut TowerState) -> TickResult {
    state.tick += 1;
    state
        .active_signals
        .retain(|s| s.expires_at_tick >= state.tick);

    let mut changed_agents = Vec::with_capacity(state.agents.len());
    let changed_floors = state.floors.iter().map(|f| f.id).collect::<Vec<_>>();
    let mut new_events = vec![];
    let mut alerts = vec![];
    let mut decision_traces = HashMap::new();

    let residential_id = state
        .floors
        .iter()
        .find(|f| f.floor_type == FloorType::Residential)
        .map(|f| f.id)
        .unwrap_or(state.floors[0].id);
    let service_id = state
        .floors
        .iter()
        .find(|f| f.floor_type == FloorType::Service)
        .map(|f| f.id)
        .unwrap_or(state.floors[0].id);
    let commercial_id = state
        .floors
        .iter()
        .find(|f| f.floor_type == FloorType::Commercial)
        .map(|f| f.id)
        .unwrap_or(state.floors[0].id);

    let mut econ = EconomyTick {
        wages_paid: 0,
        rent_collected: 0,
        service_spend: 0,
        unmet_need_penalty: 0,
    };
    let modifiers = modifiers_for_tick(state);

    for agent in &mut state.agents {
        decay_needs(&mut agent.needs, &modifiers);
        let trace = choose_goal(agent);
        let goal = trace.decision;
        decision_traces.insert(agent.id, trace);
        agent.current_goal = goal;

        match goal {
            AgentGoal::Work => {
                agent.location_floor_id = commercial_id;
                let wage = (8.0 * modifiers.wage_multiplier).round() as i32;
                agent.credits += wage;
                econ.wages_paid += i64::from(wage);
                agent.needs.energy = saturating_sub(agent.needs.energy, 5);
            }
            AgentGoal::Eat => {
                agent.location_floor_id = service_id;
                if agent.credits >= 5 {
                    agent.credits -= 5;
                    econ.service_spend += 5;
                    agent.needs.hunger = saturating_add(agent.needs.hunger, 18);
                } else {
                    econ.unmet_need_penalty += 3;
                }
            }
            AgentGoal::Rest => {
                agent.location_floor_id = residential_id;
                agent.needs.energy = saturating_add(agent.needs.energy, 16);
            }
            AgentGoal::Socialize => {
                agent.location_floor_id = service_id;
                if agent.credits >= 2 {
                    agent.credits -= 2;
                    econ.service_spend += 2;
                    agent.needs.social = saturating_add(agent.needs.social, 12);
                }
            }
            AgentGoal::Relocate => {
                agent.location_floor_id = residential_id;
                agent.needs.comfort = saturating_add(agent.needs.comfort, 10);
            }
        }

        let low_need_count = [
            agent.needs.hunger,
            agent.needs.energy,
            agent.needs.social,
            agent.needs.comfort,
        ]
        .into_iter()
        .filter(|n| *n < 25)
        .count() as i32;
        agent.mood = (agent.mood - low_need_count).clamp(0, 100);
        if low_need_count > 0 {
            econ.unmet_need_penalty += i64::from(low_need_count);
        }

        if let Some(lease_id) = agent.lease_id {
            if let Some(lease) = state.leases.get_mut(&lease_id) {
                if state.tick >= lease.due_tick && lease.status != LeaseStatus::Evicted {
                    if agent.credits >= lease.rent {
                        agent.credits -= lease.rent;
                        econ.rent_collected += i64::from(lease.rent);
                        lease.status = LeaseStatus::Active;
                        lease.due_tick = state.tick + 20;
                        agent.debt_ticks = 0;
                    } else {
                        agent.debt_ticks = agent.debt_ticks.saturating_add(1);
                        lease.status = LeaseStatus::Delinquent;
                        if agent.debt_ticks >= 3 {
                            lease.status = LeaseStatus::Evicted;
                            let event = SimEvent {
                                id: Uuid::new_v4(),
                                tick: state.tick,
                                timestamp: Utc::now(),
                                category: "lease".to_string(),
                                severity: "high".to_string(),
                                message: format!("Agent {} was evicted for non-payment", agent.id),
                                refs: vec![agent.id.to_string(), lease.id.to_string()],
                            };
                            new_events.push(event);
                            alerts.push(Alert {
                                id: Uuid::new_v4(),
                                severity: "high".to_string(),
                                category: "rent_crisis".to_string(),
                                message: "Eviction occurred due to unpaid rent".to_string(),
                                refs: vec![agent.id.to_string()],
                            });
                        }
                    }
                }
            }
        }

        if agent.needs.hunger < 10 || agent.needs.energy < 10 {
            alerts.push(Alert {
                id: Uuid::new_v4(),
                severity: "critical".to_string(),
                category: "survival".to_string(),
                message: "Critical needs detected".to_string(),
                refs: vec![agent.id.to_string()],
            });
        }

        changed_agents.push(agent.id);
    }

    state.economy = econ;
    state.alerts = dedupe_alerts(alerts.clone());
    for alert in &state.alerts {
        new_events.push(SimEvent {
            id: Uuid::new_v4(),
            tick: state.tick,
            timestamp: Utc::now(),
            category: alert.category.clone(),
            severity: alert.severity.clone(),
            message: alert.message.clone(),
            refs: alert.refs.clone(),
        });
    }

    let metrics = compute_metrics(state);
    let diff = WorldDiff {
        tick: state.tick,
        changed_agents,
        changed_floors,
        metrics_delta: metrics.clone(),
        new_events: new_events.clone(),
        runtime_assets_delta: RuntimeAssetMappingDeltas::default(),
        runtime_floor_placements_delta: RuntimeFloorPlacementDeltas::default(),
    };

    TickResult {
        diff,
        metrics,
        alerts: state.alerts.clone(),
        decision_traces,
    }
}

fn decay_needs(needs: &mut Needs, modifiers: &ActiveModifiers) {
    needs.hunger = saturating_sub(needs.hunger, modifiers.hunger_decay_step);
    needs.energy = saturating_sub(needs.energy, modifiers.energy_decay_step);
    needs.social = saturating_sub(needs.social, modifiers.social_decay_step);
    needs.comfort = saturating_sub(needs.comfort, modifiers.comfort_decay_step);
}

fn saturating_sub(v: u8, amount: u8) -> u8 {
    v.saturating_sub(amount)
}

fn saturating_add(v: u8, amount: u8) -> u8 {
    v.saturating_add(amount).min(100)
}

pub fn choose_goal(agent: &Agent) -> DecisionTrace {
    let mut scores = HashMap::new();
    scores.insert("eat".to_string(), 100 - agent.needs.hunger as i32);
    scores.insert("rest".to_string(), 100 - agent.needs.energy as i32);
    scores.insert("socialize".to_string(), 100 - agent.needs.social as i32);
    scores.insert("relocate".to_string(), 100 - agent.needs.comfort as i32);
    scores.insert(
        "work".to_string(),
        if agent.credits < 40 { 80 } else { 30 } + agent.mood / 10,
    );

    let mut selected = AgentGoal::Work;
    let mut selected_score = i32::MIN;
    for (k, score) in &scores {
        if *score > selected_score {
            selected_score = *score;
            selected = match k.as_str() {
                "eat" => AgentGoal::Eat,
                "rest" => AgentGoal::Rest,
                "socialize" => AgentGoal::Socialize,
                "relocate" => AgentGoal::Relocate,
                _ => AgentGoal::Work,
            };
        }
    }

    let mut inputs = HashMap::new();
    inputs.insert("hunger".to_string(), agent.needs.hunger as i32);
    inputs.insert("energy".to_string(), agent.needs.energy as i32);
    inputs.insert("social".to_string(), agent.needs.social as i32);
    inputs.insert("comfort".to_string(), agent.needs.comfort as i32);
    inputs.insert("credits".to_string(), agent.credits);
    inputs.insert("mood".to_string(), agent.mood);

    DecisionTrace {
        decision: selected,
        inputs,
        priority_scores: scores,
    }
}

pub fn apply_policy(
    state: &mut TowerState,
    key: String,
    value: f32,
    ttl_ticks: Option<u64>,
) -> Policy {
    let policy = Policy {
        key,
        value,
        created_at_tick: state.tick,
        expires_at_tick: ttl_ticks.map(|ttl| state.tick + ttl),
    };
    state.policies.push(policy.clone());
    policy
}

pub fn broadcast_signal(
    state: &mut TowerState,
    signal_type: String,
    target_zone: String,
    intensity: f32,
    duration_s: u64,
) -> Signal {
    let signal = Signal {
        signal_type,
        target_zone,
        intensity: intensity.clamp(0.0, 1.0),
        created_at_tick: state.tick,
        expires_at_tick: state.tick + duration_s,
    };
    state.active_signals.push(signal.clone());
    signal
}

pub fn compute_metrics(state: &TowerState) -> MetricsPoint {
    let occupancy = if state.floors.is_empty() {
        0.0
    } else {
        let total_capacity: u32 = state.floors.iter().map(|f| u32::from(f.capacity)).sum();
        if total_capacity == 0 {
            0.0
        } else {
            state.agents.len() as f32 / total_capacity as f32
        }
    };
    let avg_need_satisfaction = if state.agents.is_empty() {
        0.0
    } else {
        state
            .agents
            .iter()
            .map(|a| a.needs.avg_satisfaction())
            .sum::<f32>()
            / state.agents.len() as f32
    };
    let liquidity = if state.agents.is_empty() {
        0.0
    } else {
        state.agents.iter().map(|a| a.credits as f32).sum::<f32>() / state.agents.len() as f32
    };
    let tower_health = compute_tower_health(
        occupancy,
        avg_need_satisfaction,
        liquidity,
        state.alerts.len(),
    );

    MetricsPoint {
        tick: state.tick,
        timestamp: Utc::now(),
        occupancy,
        avg_need_satisfaction,
        liquidity,
        alert_count: state.alerts.len(),
        tower_health,
    }
}

pub fn compute_tower_health(
    occupancy: f32,
    avg_need_satisfaction: f32,
    liquidity: f32,
    alert_count: usize,
) -> f32 {
    let occupancy_score = (occupancy * 100.0).clamp(0.0, 100.0) * 0.2;
    let needs_score = avg_need_satisfaction.clamp(0.0, 100.0) * 0.45;
    let liquidity_score = (liquidity / 3.0).clamp(0.0, 100.0) * 0.25;
    let alert_penalty = (alert_count as f32 * 1.5).min(30.0);
    (occupancy_score + needs_score + liquidity_score - alert_penalty).clamp(0.0, 100.0)
}

fn dedupe_alerts(alerts: Vec<Alert>) -> Vec<Alert> {
    let mut seen = HashMap::<(String, String), Alert>::new();
    for alert in alerts {
        let key = (alert.category.clone(), alert.message.clone());
        seen.entry(key).or_insert(alert);
    }
    seen.into_values().collect()
}

#[derive(Debug, Clone, Copy)]
struct ActiveModifiers {
    hunger_decay_step: u8,
    energy_decay_step: u8,
    social_decay_step: u8,
    comfort_decay_step: u8,
    wage_multiplier: f32,
}

fn modifiers_for_tick(state: &TowerState) -> ActiveModifiers {
    let mut hunger_decay_step = 2.0;
    let mut energy_decay_step = 2.0;
    let mut social_decay_step = 1.0;
    let mut comfort_decay_step = 1.0;
    let mut wage_multiplier = 1.0;

    for policy in &state.policies {
        if policy.expires_at_tick.is_some_and(|exp| exp < state.tick) {
            continue;
        }
        match policy.key.as_str() {
            "food_subsidy" => hunger_decay_step *= 1.0 - policy.value.clamp(0.0, 0.8),
            "rest_efficiency" => energy_decay_step *= 1.0 - policy.value.clamp(0.0, 0.8),
            "social_program" => social_decay_step *= 1.0 - policy.value.clamp(0.0, 0.8),
            "comfort_maintenance" => comfort_decay_step *= 1.0 - policy.value.clamp(0.0, 0.8),
            "wage_boost" => wage_multiplier *= 1.0 + policy.value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    for signal in &state.active_signals {
        match signal.signal_type.as_str() {
            "crisis_food" => hunger_decay_step *= 1.0 - (signal.intensity * 0.5),
            "focus_work" => wage_multiplier *= 1.0 + (signal.intensity * 0.2),
            "social_rally" => social_decay_step *= 1.0 - (signal.intensity * 0.6),
            _ => {}
        }
    }

    ActiveModifiers {
        hunger_decay_step: hunger_decay_step.max(0.0).round() as u8,
        energy_decay_step: energy_decay_step.max(0.0).round() as u8,
        social_decay_step: social_decay_step.max(0.0).round() as u8,
        comfort_decay_step: comfort_decay_step.max(0.0).round() as u8,
        wage_multiplier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn need_decay_and_thresholds_work() {
        let mut needs = Needs {
            hunger: 20,
            energy: 20,
            social: 20,
            comfort: 20,
        };
        decay_needs(
            &mut needs,
            &ActiveModifiers {
                hunger_decay_step: 2,
                energy_decay_step: 2,
                social_decay_step: 1,
                comfort_decay_step: 1,
                wage_multiplier: 1.0,
            },
        );
        assert_eq!(needs.hunger, 18);
        assert_eq!(needs.energy, 18);
        assert_eq!(needs.social, 19);
        assert_eq!(needs.comfort, 19);
    }

    #[test]
    fn goal_priority_is_deterministic() {
        let agent = Agent {
            id: Uuid::new_v4(),
            role: "worker".to_string(),
            credits: 10,
            needs: Needs {
                hunger: 5,
                energy: 90,
                social: 80,
                comfort: 80,
            },
            mood: 70,
            current_goal: AgentGoal::Work,
            location_floor_id: Uuid::new_v4(),
            lease_id: None,
            debt_ticks: 0,
        };
        let one = choose_goal(&agent);
        let two = choose_goal(&agent);
        assert_eq!(one.decision, AgentGoal::Eat);
        assert_eq!(one.decision, two.decision);
        assert_eq!(one.priority_scores, two.priority_scores);
    }

    #[test]
    fn rent_debt_eviction_path_triggers() {
        let mut tower = initialize_tower(42);
        let first = tower.agents.first_mut().expect("agent exists");
        first.credits = 0;
        first.debt_ticks = 2;
        if let Some(lease_id) = first.lease_id {
            if let Some(lease) = tower.leases.get_mut(&lease_id) {
                lease.due_tick = 1;
                lease.status = LeaseStatus::Delinquent;
            }
        }

        let tick = run_tick(&mut tower);
        let first_after = tower.agents.first().expect("agent exists");
        let lease_id = first_after.lease_id.expect("lease exists");
        let lease = tower.leases.get(&lease_id).expect("lease exists");
        assert_eq!(lease.status, LeaseStatus::Evicted);
        assert!(tick.diff.new_events.iter().any(|e| e.category == "lease"));
    }

    #[test]
    fn health_score_penalizes_alerts() {
        let good = compute_tower_health(0.8, 90.0, 250.0, 0);
        let bad = compute_tower_health(0.8, 90.0, 250.0, 20);
        assert!(good > bad);
    }

    #[test]
    fn policy_and_signal_modify_modifiers() {
        let mut tower = initialize_tower(7);
        apply_policy(&mut tower, "wage_boost".to_string(), 0.5, Some(10));
        broadcast_signal(
            &mut tower,
            "focus_work".to_string(),
            "all".to_string(),
            1.0,
            5,
        );
        let m = modifiers_for_tick(&tower);
        assert!(m.wage_multiplier > 1.0);
    }

    #[test]
    fn boundary_wall_resolver_uses_semantic_ids() {
        assert_eq!(
            resolve_boundary_wall_tile(0, 0, 10, 7),
            Some("tile_wall_corner_top_left")
        );
        assert_eq!(
            resolve_boundary_wall_tile(9, 0, 10, 7),
            Some("tile_wall_corner_top_right")
        );
        assert_eq!(
            resolve_boundary_wall_tile(0, 6, 10, 7),
            Some("tile_wall_corner_bottom_left")
        );
        assert_eq!(
            resolve_boundary_wall_tile(9, 6, 10, 7),
            Some("tile_wall_corner_bottom_right")
        );
        assert_eq!(
            resolve_boundary_wall_tile(4, 0, 10, 7),
            Some("tile_wall_edge_top")
        );
        assert_eq!(
            resolve_boundary_wall_tile(4, 6, 10, 7),
            Some("tile_wall_edge_bottom")
        );
        assert_eq!(
            resolve_boundary_wall_tile(0, 3, 10, 7),
            Some("tile_wall_edge_left")
        );
        assert_eq!(
            resolve_boundary_wall_tile(9, 3, 10, 7),
            Some("tile_wall_edge_right")
        );
        assert_eq!(resolve_boundary_wall_tile(4, 3, 10, 7), None);
    }

    #[test]
    fn floor_tilemap_avoids_raw_wall_ids() {
        let map = build_floor_tilemap(FloorType::Commercial);
        assert_eq!(map.width, 14);
        assert_eq!(map.height, 10);
        assert!(map.tiles.iter().any(|id| id == "tile_wall_corner_top_left"));
        assert!(map.tiles.iter().any(|id| id == "tile_wall_terminal"));
        assert!(map.tiles.iter().all(|id| !id.starts_with("tile_wall_r")));
    }

    #[test]
    fn ensure_floor_tilemaps_backfills_legacy_snapshots() {
        let mut tower = initialize_tower(11);
        tower.floors[0].tilemap = FloorTileMap::default();
        ensure_floor_tilemaps(&mut tower);
        let map = &tower.floors[0].tilemap;
        assert!(map.width > 0);
        assert!(map.height > 0);
        assert_eq!(map.tiles.len(), map.width as usize * map.height as usize);
        assert!(map.tiles.iter().any(|id| id.starts_with("tile_wall_")));
    }
}
