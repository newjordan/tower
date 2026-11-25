use chrono::{DateTime, Utc};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use uuid::Uuid;

pub const MAX_AGENTS: usize = 100;
pub const DEFAULT_TOWER_ID: &str = "tower-1";
const FLOOR_TEMPLATE_CATALOG_JSON: &str =
    include_str!("../assets/templates/floor_templates_v1.json");

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FloorType {
    Residential,
    Commercial,
    Service,
    Utility,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum FloorStage {
    #[default]
    Seed,
    Growing,
    Mature,
    Overloaded,
}

#[derive(Debug, Clone, Deserialize)]
struct FloorTemplateCatalog {
    #[serde(default)]
    settings: FloorTemplateSettings,
    #[serde(default)]
    templates: Vec<FloorTemplateSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct FloorTemplateSpec {
    template_id: String,
    floor_type: FloorType,
    stage: FloorStage,
    capacity_multiplier_pct: u16,
    upkeep_multiplier_pct: i32,
    rent_curve_delta: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FloorTemplateSettings {
    #[serde(default)]
    stage_thresholds: StageThresholdSettings,
    #[serde(default)]
    expansion: ExpansionSettings,
}

#[derive(Debug, Clone, Deserialize)]
struct StageThresholdSettings {
    #[serde(default = "default_growing_min_score")]
    growing_min_score: u8,
    #[serde(default = "default_mature_min_score")]
    mature_min_score: u8,
    #[serde(default = "default_overloaded_min_score")]
    overloaded_min_score: u8,
}

impl Default for StageThresholdSettings {
    fn default() -> Self {
        Self {
            growing_min_score: default_growing_min_score(),
            mature_min_score: default_mature_min_score(),
            overloaded_min_score: default_overloaded_min_score(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ExpansionSettings {
    #[serde(default = "default_max_floors")]
    max_floors: usize,
    #[serde(default = "default_build_cooldown_ticks")]
    build_cooldown_ticks: u64,
    #[serde(default = "default_min_avg_stage_score")]
    min_avg_stage_score: u8,
}

impl Default for ExpansionSettings {
    fn default() -> Self {
        Self {
            max_floors: default_max_floors(),
            build_cooldown_ticks: default_build_cooldown_ticks(),
            min_avg_stage_score: default_min_avg_stage_score(),
        }
    }
}

#[derive(Debug, Clone)]
struct FloorTemplateCatalogResolved {
    settings: FloorTemplateSettings,
    templates: Vec<FloorTemplateSpec>,
}

fn default_growing_min_score() -> u8 {
    35
}

fn default_mature_min_score() -> u8 {
    65
}

fn default_overloaded_min_score() -> u8 {
    90
}

fn default_max_floors() -> usize {
    8
}

fn default_build_cooldown_ticks() -> u64 {
    120
}

fn default_min_avg_stage_score() -> u8 {
    30
}

static FLOOR_TEMPLATE_CATALOG: OnceLock<FloorTemplateCatalogResolved> = OnceLock::new();

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    WorkArea,
    BreakRoom,
    Reception,
    Utility,
    Corridor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfficeZone {
    pub kind: ZoneKind,
    pub x_min: u8,
    pub y_min: u8,
    pub x_max: u8,
    pub y_max: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FurniturePlacement {
    pub asset_id: String,
    pub x: u8,
    pub y: u8,
    pub zone: ZoneKind,
    pub placed_at_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OfficeLayout {
    #[serde(default)]
    pub furniture: Vec<FurniturePlacement>,
    #[serde(default)]
    pub zones: Vec<OfficeZone>,
    #[serde(default)]
    pub max_slots: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum ScriptConfig {
    #[serde(rename = "layout_bias")]
    LayoutBias { work_pct: f32, break_pct: f32 },
    #[serde(rename = "pipeline_install")]
    PipelineInstall { pipeline_kind: PipelineKind },
    #[serde(rename = "furniture_upgrade")]
    FurnitureUpgrade { tier: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagerScript {
    pub script_id: String,
    pub installed_at_tick: u64,
    pub config: ScriptConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PipelineKind {
    CiCd,
    MailRoom,
    FoodService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineStage {
    pub name: String,
    pub asset_id: String,
    pub x: u8,
    pub y: u8,
    pub duration_ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pipeline {
    pub kind: PipelineKind,
    pub stages: Vec<PipelineStage>,
    pub current_stage_idx: usize,
    pub progress_ticks: u16,
    pub total_completions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FloorManager {
    #[serde(default)]
    pub age_ticks: u64,
    #[serde(default)]
    pub placement_budget: u16,
    #[serde(default)]
    pub layout: OfficeLayout,
    #[serde(default)]
    pub scripts: Vec<ManagerScript>,
    #[serde(default)]
    pub pipeline: Option<Pipeline>,
    #[serde(default)]
    pub construction_fx: Vec<ConstructionFx>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstructionFx {
    pub x: u8,
    pub y: u8,
    pub expires_at_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Floor {
    pub id: Uuid,
    pub name: String,
    pub floor_type: FloorType,
    #[serde(default)]
    pub template_id: String,
    #[serde(default)]
    pub stage: FloorStage,
    #[serde(default)]
    pub stage_score: u8,
    #[serde(default)]
    pub base_capacity: u16,
    #[serde(default)]
    pub base_upkeep: i32,
    #[serde(default)]
    pub base_rent_curve: i32,
    pub capacity: u16,
    pub occupancy: u16,
    pub upkeep: i32,
    pub rent_curve: i32,
    #[serde(default)]
    pub tilemap: FloorTileMap,
    #[serde(default)]
    pub manager: FloorManager,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action_type")]
pub enum DirectiveAction {
    #[serde(rename = "set_layout_bias")]
    SetLayoutBias { work_pct: f32, break_pct: f32 },
    #[serde(rename = "install_script")]
    InstallScript {
        script_id: String,
        config: ScriptConfig,
    },
    #[serde(rename = "install_pipeline")]
    InstallPipeline { kind: PipelineKind },
    #[serde(rename = "upgrade_furniture")]
    UpgradeFurniture,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorDirective {
    pub target_floor_id: Uuid,
    pub action: DirectiveAction,
    pub issued_at_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct OperatorAgent {
    #[serde(default)]
    pub budget: i32,
    #[serde(default)]
    pub next_review_tick: u64,
    #[serde(default)]
    pub pending_directives: Vec<OperatorDirective>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TowerState {
    pub tower_id: String,
    pub tick: u64,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub next_floor_build_tick: u64,
    #[serde(default)]
    pub expansion_count: u32,
    pub agents: Vec<Agent>,
    pub floors: Vec<Floor>,
    pub leases: HashMap<Uuid, Lease>,
    pub economy: EconomyTick,
    pub alerts: Vec<Alert>,
    pub policies: Vec<Policy>,
    pub active_signals: Vec<Signal>,
    #[serde(default)]
    pub operator: OperatorAgent,
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
        if floor.template_id.trim().is_empty() {
            floor.template_id = default_floor_template_id(floor.floor_type, floor.stage);
        }
        if floor.base_capacity == 0 {
            floor.base_capacity = floor.capacity.max(1);
        }
        if floor.base_upkeep == 0 {
            floor.base_upkeep = floor.upkeep.max(1);
        }
        if floor.base_rent_curve == 0 {
            floor.base_rent_curve = floor.rent_curve.max(1);
        }
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
        template_id: "residential_seed_v1".to_string(),
        stage: FloorStage::Seed,
        stage_score: 0,
        base_capacity: 120,
        base_upkeep: 30,
        base_rent_curve: 18,
        capacity: 120,
        occupancy: MAX_AGENTS as u16,
        upkeep: 30,
        rent_curve: 18,
        tilemap: build_floor_tilemap(FloorType::Residential),
        manager: FloorManager::default(),
    });
    floors.push(Floor {
        id: service_id,
        name: "Food Court".to_string(),
        floor_type: FloorType::Service,
        template_id: "service_seed_v1".to_string(),
        stage: FloorStage::Seed,
        stage_score: 0,
        base_capacity: 80,
        base_upkeep: 22,
        base_rent_curve: 10,
        capacity: 80,
        occupancy: 0,
        upkeep: 22,
        rent_curve: 10,
        tilemap: build_floor_tilemap(FloorType::Service),
        manager: FloorManager::default(),
    });
    floors.push(Floor {
        id: commercial_id,
        name: "Work Hub".to_string(),
        floor_type: FloorType::Commercial,
        template_id: "commercial_seed_v1".to_string(),
        stage: FloorStage::Seed,
        stage_score: 0,
        base_capacity: 100,
        base_upkeep: 40,
        base_rent_curve: 12,
        capacity: 100,
        occupancy: 0,
        upkeep: 40,
        rent_curve: 12,
        tilemap: build_floor_tilemap(FloorType::Commercial),
        manager: FloorManager::default(),
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
        next_floor_build_tick: 120,
        expansion_count: 0,
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
        operator: OperatorAgent::default(),
    };
    ensure_floor_tilemaps(&mut state);
    refresh_floor_progression(&mut state, &mut vec![]);
    state
}

pub fn run_tick(state: &mut TowerState) -> TickResult {
    state.tick += 1;
    ensure_floor_tilemaps(state);
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

        if let Some(lease_id) = agent.lease_id
            && let Some(lease) = state.leases.get_mut(&lease_id)
            && state.tick >= lease.due_tick
            && lease.status != LeaseStatus::Evicted
        {
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

    // Floor manager ticks
    for floor in &mut state.floors {
        tick_floor_manager(floor, state.tick, &mut new_events);
    }

    // Operator review + apply directives
    operator_review(state);
    apply_pending_directives(state);

    refresh_floor_progression(state, &mut new_events);

    // Enforce furniture cap after stage changes (stages can regress)
    for floor in &mut state.floors {
        let max = max_slots_for_stage(floor.floor_type, floor.stage) as usize;
        while floor.manager.layout.furniture.len() > max {
            floor.manager.layout.furniture.pop();
        }
    }

    try_expand_tower(state, &mut new_events);
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

fn fallback_floor_template_id(floor_type: FloorType, stage: FloorStage) -> &'static str {
    match (floor_type, stage) {
        (FloorType::Residential, FloorStage::Seed) => "residential_seed_v1",
        (FloorType::Residential, FloorStage::Growing) => "residential_growing_v1",
        (FloorType::Residential, FloorStage::Mature) => "residential_mature_v1",
        (FloorType::Residential, FloorStage::Overloaded) => "residential_overloaded_v1",
        (FloorType::Commercial, FloorStage::Seed) => "commercial_seed_v1",
        (FloorType::Commercial, FloorStage::Growing) => "commercial_growing_v1",
        (FloorType::Commercial, FloorStage::Mature) => "commercial_mature_v1",
        (FloorType::Commercial, FloorStage::Overloaded) => "commercial_overloaded_v1",
        (FloorType::Service, FloorStage::Seed) => "service_seed_v1",
        (FloorType::Service, FloorStage::Growing) => "service_growing_v1",
        (FloorType::Service, FloorStage::Mature) => "service_mature_v1",
        (FloorType::Service, FloorStage::Overloaded) => "service_overloaded_v1",
        (FloorType::Utility, FloorStage::Seed) => "utility_seed_v1",
        (FloorType::Utility, FloorStage::Growing) => "utility_growing_v1",
        (FloorType::Utility, FloorStage::Mature) => "utility_mature_v1",
        (FloorType::Utility, FloorStage::Overloaded) => "utility_overloaded_v1",
    }
}

fn floor_template_catalog() -> &'static FloorTemplateCatalogResolved {
    FLOOR_TEMPLATE_CATALOG.get_or_init(|| {
        serde_json::from_str::<FloorTemplateCatalog>(FLOOR_TEMPLATE_CATALOG_JSON)
            .map(|catalog| FloorTemplateCatalogResolved {
                settings: catalog.settings,
                templates: catalog.templates,
            })
            .unwrap_or_else(|_| FloorTemplateCatalogResolved {
                settings: FloorTemplateSettings::default(),
                templates: vec![],
            })
    })
}

fn find_floor_template(
    floor_type: FloorType,
    stage: FloorStage,
) -> Option<&'static FloorTemplateSpec> {
    floor_template_catalog()
        .templates
        .iter()
        .find(|spec| spec.floor_type == floor_type && spec.stage == stage)
}

fn floor_template_settings() -> &'static FloorTemplateSettings {
    &floor_template_catalog().settings
}

fn default_floor_template_id(floor_type: FloorType, stage: FloorStage) -> String {
    find_floor_template(floor_type, stage)
        .map(|spec| spec.template_id.clone())
        .unwrap_or_else(|| fallback_floor_template_id(floor_type, stage).to_string())
}

fn stage_capacity_multiplier_pct(stage: FloorStage) -> u16 {
    match stage {
        FloorStage::Seed => 100,
        FloorStage::Growing => 112,
        FloorStage::Mature => 125,
        FloorStage::Overloaded => 108,
    }
}

fn stage_rent_curve_delta(stage: FloorStage) -> i32 {
    match stage {
        FloorStage::Seed => 0,
        FloorStage::Growing => 1,
        FloorStage::Mature => 3,
        FloorStage::Overloaded => 2,
    }
}

fn stage_upkeep_multiplier_pct(stage: FloorStage) -> i32 {
    match stage {
        FloorStage::Seed => 100,
        FloorStage::Growing => 106,
        FloorStage::Mature => 114,
        FloorStage::Overloaded => 130,
    }
}

fn stage_from_score(score: u8) -> FloorStage {
    let thresholds = &floor_template_settings().stage_thresholds;
    if score >= thresholds.overloaded_min_score {
        FloorStage::Overloaded
    } else if score >= thresholds.mature_min_score {
        FloorStage::Mature
    } else if score >= thresholds.growing_min_score {
        FloorStage::Growing
    } else {
        FloorStage::Seed
    }
}

fn floor_stage_label(stage: FloorStage) -> &'static str {
    match stage {
        FloorStage::Seed => "seed",
        FloorStage::Growing => "growing",
        FloorStage::Mature => "mature",
        FloorStage::Overloaded => "overloaded",
    }
}

fn refresh_floor_progression(state: &mut TowerState, out_events: &mut Vec<SimEvent>) {
    let mut assigned_agents: HashMap<Uuid, usize> = HashMap::new();
    let mut mood_by_floor: HashMap<Uuid, (i64, usize)> = HashMap::new();
    for agent in &state.agents {
        *assigned_agents.entry(agent.location_floor_id).or_default() += 1;
        let entry = mood_by_floor.entry(agent.location_floor_id).or_default();
        entry.0 += i64::from(agent.mood.clamp(0, 100));
        entry.1 += 1;
    }
    let critical_alerts = state
        .alerts
        .iter()
        .filter(|a| a.severity == "critical")
        .count() as i32;
    let unmet_penalty = state.economy.unmet_need_penalty.clamp(0, 120) as i32;

    for floor in &mut state.floors {
        let occupants = assigned_agents.get(&floor.id).copied().unwrap_or_default();
        floor.occupancy = occupants.min(u16::MAX as usize) as u16;
        let base_capacity = floor.base_capacity.max(1) as i32;
        let load_score = ((occupants as i32 * 100) / base_capacity).clamp(0, 100);
        let avg_mood = mood_by_floor
            .get(&floor.id)
            .map(|(sum, count)| (*sum as f32 / (*count as f32)).round() as i32)
            .unwrap_or(70);
        let mood_stress = ((100 - avg_mood).clamp(0, 100) / 3).clamp(0, 25);
        let alert_stress = (critical_alerts * 4).clamp(0, 20);
        let need_stress = (unmet_penalty / 12).clamp(0, 10);
        let stage_score =
            (load_score + mood_stress + alert_stress + need_stress).clamp(0, 100) as u8;
        let next_stage = stage_from_score(stage_score);
        if next_stage != floor.stage {
            out_events.push(SimEvent {
                id: Uuid::new_v4(),
                tick: state.tick,
                timestamp: Utc::now(),
                category: "floor_progression".to_string(),
                severity: if next_stage > floor.stage {
                    "info".to_string()
                } else {
                    "low".to_string()
                },
                message: format!(
                    "{} transitioned {} -> {}",
                    floor.name,
                    floor_stage_label(floor.stage),
                    floor_stage_label(next_stage)
                ),
                refs: vec![
                    floor.id.to_string(),
                    floor.name.clone(),
                    floor.template_id.clone(),
                ],
            });
        }
        floor.stage = next_stage;
        floor.stage_score = stage_score;
        let template = find_floor_template(floor.floor_type, floor.stage);
        floor.template_id = template
            .map(|spec| spec.template_id.clone())
            .unwrap_or_else(|| default_floor_template_id(floor.floor_type, floor.stage));
        let capacity_pct = template
            .map(|spec| spec.capacity_multiplier_pct)
            .unwrap_or_else(|| stage_capacity_multiplier_pct(floor.stage));
        let upkeep_pct = template
            .map(|spec| spec.upkeep_multiplier_pct)
            .unwrap_or_else(|| stage_upkeep_multiplier_pct(floor.stage));
        let rent_delta = template
            .map(|spec| spec.rent_curve_delta)
            .unwrap_or_else(|| stage_rent_curve_delta(floor.stage));
        floor.capacity = ((u32::from(floor.base_capacity.max(1)) * u32::from(capacity_pct)) / 100)
            .max(1)
            .min(u32::from(u16::MAX)) as u16;
        floor.upkeep = ((floor.base_upkeep.max(1) * upkeep_pct) / 100).max(1);
        floor.rent_curve = (floor.base_rent_curve.max(1) + rent_delta).max(1);
    }
}

fn seed_floor_stats_for_type(floor_type: FloorType) -> (u16, i32, i32) {
    match floor_type {
        FloorType::Residential => (120, 30, 18),
        FloorType::Commercial => (90, 36, 14),
        FloorType::Service => (70, 24, 11),
        FloorType::Utility => (60, 28, 9),
    }
}

fn floor_type_build_label(floor_type: FloorType) -> &'static str {
    match floor_type {
        FloorType::Residential => "Residences",
        FloorType::Commercial => "Work Hub",
        FloorType::Service => "Food Court",
        FloorType::Utility => "Utility",
    }
}

fn floor_type_tie_rank(floor_type: FloorType) -> u8 {
    match floor_type {
        FloorType::Residential => 0,
        FloorType::Commercial => 1,
        FloorType::Service => 2,
        FloorType::Utility => 3,
    }
}

fn floor_type_ref_label(floor_type: FloorType) -> &'static str {
    match floor_type {
        FloorType::Residential => "residential",
        FloorType::Commercial => "commercial",
        FloorType::Service => "service",
        FloorType::Utility => "utility",
    }
}

fn next_expansion_floor_type(state: &TowerState) -> FloorType {
    let mut overloaded_by_type: HashMap<FloorType, usize> = HashMap::new();
    for floor in &state.floors {
        if floor.stage == FloorStage::Overloaded {
            *overloaded_by_type.entry(floor.floor_type).or_default() += 1;
        }
    }
    // Equal overload counts used to follow HashMap iteration order, which is
    // reseeded per map. Rank is a total tie-break so the same tower always
    // expands the same way. A strictly higher count still wins.
    let dominant = overloaded_by_type
        .into_iter()
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| floor_type_tie_rank(left.0).cmp(&floor_type_tie_rank(right.0)))
        })
        .map(|(floor_type, _)| floor_type)
        .unwrap_or(FloorType::Commercial);
    match dominant {
        FloorType::Commercial => FloorType::Service,
        FloorType::Service => FloorType::Utility,
        FloorType::Utility => FloorType::Commercial,
        FloorType::Residential => FloorType::Commercial,
    }
}

fn try_expand_tower(state: &mut TowerState, out_events: &mut Vec<SimEvent>) {
    let expansion = &floor_template_settings().expansion;
    let max_floors = expansion.max_floors.max(1);
    let build_cooldown_ticks = expansion.build_cooldown_ticks.max(1);
    let min_avg_stage_score = f32::from(expansion.min_avg_stage_score.min(100));
    if state.tick < state.next_floor_build_tick {
        return;
    }
    if state.floors.len() >= max_floors {
        return;
    }
    let overloaded = state
        .floors
        .iter()
        .filter(|floor| floor.stage == FloorStage::Overloaded)
        .count();
    if overloaded == 0 {
        return;
    }
    let avg_stage_score = if state.floors.is_empty() {
        0.0
    } else {
        state
            .floors
            .iter()
            .map(|floor| f32::from(floor.stage_score))
            .sum::<f32>()
            / state.floors.len() as f32
    };
    if avg_stage_score < min_avg_stage_score {
        return;
    }

    let floor_type = next_expansion_floor_type(state);
    let (base_capacity, base_upkeep, base_rent_curve) = seed_floor_stats_for_type(floor_type);
    let floor = Floor {
        id: Uuid::new_v4(),
        name: format!(
            "{} Annex {}",
            floor_type_build_label(floor_type),
            state.expansion_count + 1
        ),
        floor_type,
        template_id: default_floor_template_id(floor_type, FloorStage::Seed),
        stage: FloorStage::Seed,
        stage_score: 0,
        base_capacity,
        base_upkeep,
        base_rent_curve,
        capacity: base_capacity,
        occupancy: 0,
        upkeep: base_upkeep,
        rent_curve: base_rent_curve,
        tilemap: build_floor_tilemap(floor_type),
        manager: FloorManager::default(),
    };
    state.floors.push(floor.clone());
    state.expansion_count = state.expansion_count.saturating_add(1);
    state.next_floor_build_tick = state.tick.saturating_add(build_cooldown_ticks);
    out_events.push(SimEvent {
        id: Uuid::new_v4(),
        tick: state.tick,
        timestamp: Utc::now(),
        category: "floor_construction".to_string(),
        severity: "info".to_string(),
        message: format!("Constructed {}", floor.name),
        refs: vec![
            floor.id.to_string(),
            floor.name,
            floor_type_ref_label(floor.floor_type).to_string(),
        ],
    });
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
    let eat = 100 - i32::from(agent.needs.hunger);
    let rest = 100 - i32::from(agent.needs.energy);
    let socialize = 100 - i32::from(agent.needs.social);
    let relocate = 100 - i32::from(agent.needs.comfort);
    let work = if agent.credits < 40 { 80 } else { 30 } + agent.mood / 10;

    let mut scores = HashMap::new();
    scores.insert("eat".to_string(), eat);
    scores.insert("rest".to_string(), rest);
    scores.insert("socialize".to_string(), socialize);
    scores.insert("relocate".to_string(), relocate);
    scores.insert("work".to_string(), work);

    // Strictly-greater selection. Equal scores keep the earlier candidate so a
    // given needs vector always resolves to the same goal. HashMap iteration
    // is not stable across runs, so it cannot be the tie-break.
    let mut selected = AgentGoal::Work;
    let mut selected_score = i32::MIN;
    for (score, goal) in [
        (eat, AgentGoal::Eat),
        (rest, AgentGoal::Rest),
        (socialize, AgentGoal::Socialize),
        (relocate, AgentGoal::Relocate),
        (work, AgentGoal::Work),
    ] {
        if score > selected_score {
            selected_score = score;
            selected = goal;
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

// --- Floor Manager, Operator, Scripts, Pipelines ---

const FURNITURE_COST: u16 = 5;
const FURNITURE_BUDGET_CAP: u16 = 30;
const OPERATOR_REVIEW_INTERVAL: u64 = 40;

const WORK_AREA_PALETTE: &[&str] = &[
    "prop_lpc_desk_ornate",
    "prop_lpc_laptop",
    "prop_lpc_copy_machine",
];
const BREAK_ROOM_PALETTE: &[&str] = &[
    "prop_lpc_coffee_maker",
    "prop_lpc_water_cooler",
    "prop_lpc_card_table",
];
const RECEPTION_PALETTE: &[&str] = &[
    "prop_lpc_mailboxes",
    "prop_lpc_bins",
    "prop_lpc_office_portraits",
];
const UPGRADE_PALETTE: &[&str] = &["prop_lpc_tv_widescreen", "prop_lpc_desk_ornate"];

fn max_slots_for_stage(floor_type: FloorType, stage: FloorStage) -> u8 {
    match (floor_type, stage) {
        (_, FloorStage::Seed) => 2,
        (_, FloorStage::Growing) => 4,
        (_, FloorStage::Mature) => 6,
        (_, FloorStage::Overloaded) => 8,
    }
}

fn walkable_cells_for_tilemap(tilemap: &FloorTileMap) -> Vec<(u8, u8)> {
    let width = tilemap.width;
    let height = tilemap.height;
    let mut cells = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let idx = usize::from(y) * usize::from(width) + usize::from(x);
            let is_walkable = tilemap
                .tiles
                .get(idx)
                .is_some_and(|tile| !tile.starts_with("tile_wall_"));
            if is_walkable {
                cells.push((x, y));
            }
        }
    }
    cells
}

fn compute_zones(
    tilemap: &FloorTileMap,
    walkable_cells: &[(u8, u8)],
    layout_bias: Option<(f32, f32)>,
) -> Vec<OfficeZone> {
    let w = tilemap.width;
    let h = tilemap.height;
    if w < 3 || h < 3 {
        let zone = OfficeZone {
            kind: ZoneKind::WorkArea,
            x_min: 0,
            y_min: 0,
            x_max: w.saturating_sub(1),
            y_max: h.saturating_sub(1),
        };
        if walkable_cells.iter().any(|&(cx, cy)| {
            cx >= zone.x_min && cx <= zone.x_max && cy >= zone.y_min && cy <= zone.y_max
        }) {
            return vec![zone];
        } else {
            return vec![];
        }
    }
    // work_pct controls how much vertical space WorkArea gets (top portion)
    // break_pct controls how much horizontal space BreakRoom gets (bottom-left portion)
    let (work_pct, break_pct) = layout_bias.unwrap_or((0.5, 0.5));
    let mid_y = (h as f32 * work_pct).round().max(2.0).min((h - 2) as f32) as u8;
    let mid_x = (w as f32 * break_pct).round().max(2.0).min((w - 2) as f32) as u8;
    let candidates = vec![
        OfficeZone {
            kind: ZoneKind::WorkArea,
            x_min: 1,
            y_min: 1,
            x_max: w.saturating_sub(2),
            y_max: mid_y.saturating_sub(1),
        },
        OfficeZone {
            kind: ZoneKind::BreakRoom,
            x_min: 1,
            y_min: mid_y + 1,
            x_max: mid_x.saturating_sub(1),
            y_max: h.saturating_sub(2),
        },
        OfficeZone {
            kind: ZoneKind::Reception,
            x_min: mid_x + 1,
            y_min: mid_y + 1,
            x_max: w.saturating_sub(2),
            y_max: h.saturating_sub(2),
        },
        OfficeZone {
            kind: ZoneKind::Corridor,
            x_min: mid_x.saturating_sub(1),
            y_min: 0,
            x_max: mid_x,
            y_max: h.saturating_sub(1),
        },
    ];
    candidates
        .into_iter()
        .filter(|zone| {
            walkable_cells.iter().any(|&(cx, cy)| {
                cx >= zone.x_min && cx <= zone.x_max && cy >= zone.y_min && cy <= zone.y_max
            })
        })
        .collect()
}

fn palette_for_zone(zone: ZoneKind, upgrade_tier: u8) -> &'static [&'static str] {
    if upgrade_tier > 0 {
        return UPGRADE_PALETTE;
    }
    match zone {
        ZoneKind::WorkArea => WORK_AREA_PALETTE,
        ZoneKind::BreakRoom => BREAK_ROOM_PALETTE,
        ZoneKind::Reception => RECEPTION_PALETTE,
        ZoneKind::Utility => WORK_AREA_PALETTE,
        ZoneKind::Corridor => WORK_AREA_PALETTE,
    }
}

fn cells_in_zone(zone: &OfficeZone) -> Vec<(u8, u8)> {
    let mut cells = Vec::new();
    for y in zone.y_min..=zone.y_max {
        for x in zone.x_min..=zone.x_max {
            cells.push((x, y));
        }
    }
    cells
}

fn upgrade_tier_from_scripts(scripts: &[ManagerScript]) -> u8 {
    scripts
        .iter()
        .filter_map(|s| match &s.config {
            ScriptConfig::FurnitureUpgrade { tier } => Some(*tier),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn layout_bias_from_scripts(scripts: &[ManagerScript]) -> Option<(f32, f32)> {
    scripts.iter().rev().find_map(|s| match &s.config {
        ScriptConfig::LayoutBias {
            work_pct,
            break_pct,
        } => Some((*work_pct, *break_pct)),
        _ => None,
    })
}

pub fn tick_floor_manager(floor: &mut Floor, tick: u64, new_events: &mut Vec<SimEvent>) {
    let mgr = &mut floor.manager;
    mgr.age_ticks += 1;
    mgr.placement_budget = (mgr.placement_budget + 1).min(FURNITURE_BUDGET_CAP);

    // Expire construction FX
    mgr.construction_fx.retain(|fx| fx.expires_at_tick > tick);

    // Recompute zones (cheap, ensures layout bias changes take effect)
    let walkable = walkable_cells_for_tilemap(&floor.tilemap);
    let bias = layout_bias_from_scripts(&mgr.scripts);
    mgr.layout.zones = compute_zones(&floor.tilemap, &walkable, bias);
    mgr.layout.max_slots = max_slots_for_stage(floor.floor_type, floor.stage);

    // Trim furniture if stage regressed and we're over the new cap
    while mgr.layout.furniture.len() > mgr.layout.max_slots as usize {
        // Remove the most recently placed item
        mgr.layout.furniture.pop();
    }

    // Try to place furniture
    if mgr.placement_budget >= FURNITURE_COST
        && (mgr.layout.furniture.len() as u8) < mgr.layout.max_slots
    {
        let placeable_zones: Vec<&OfficeZone> = mgr
            .layout
            .zones
            .iter()
            .filter(|z| z.kind != ZoneKind::Corridor)
            .collect();
        if !placeable_zones.is_empty() {
            // Round-robin biased toward WorkArea
            let furniture_count = mgr.layout.furniture.len();
            let zone_idx = if furniture_count.is_multiple_of(3) || furniture_count % 3 == 1 {
                // WorkArea bias: pick first non-corridor (usually WorkArea)
                0
            } else {
                (furniture_count / 2) % placeable_zones.len()
            };
            let zone = placeable_zones[zone_idx % placeable_zones.len()];
            let zone_kind = zone.kind;
            let upgrade_tier = upgrade_tier_from_scripts(&mgr.scripts);
            let palette = palette_for_zone(zone_kind, upgrade_tier);
            if !palette.is_empty() {
                let asset_idx = furniture_count % palette.len();
                let asset_id = palette[asset_idx].to_string();
                let zone_cells = cells_in_zone(zone);
                let occupied: std::collections::HashSet<(u8, u8)> =
                    mgr.layout.furniture.iter().map(|f| (f.x, f.y)).collect();
                let mut free_cells: Vec<(u8, u8)> = zone_cells
                    .into_iter()
                    .filter(|c| !occupied.contains(c))
                    .collect();
                // Pick the cell that maximizes minimum distance from existing furniture
                if !free_cells.is_empty() && !occupied.is_empty() {
                    free_cells.sort_by(|a, b| {
                        let a_min = occupied
                            .iter()
                            .map(|o| u16::from(a.0.abs_diff(o.0)) + u16::from(a.1.abs_diff(o.1)))
                            .min()
                            .unwrap_or(u16::MAX);
                        let b_min = occupied
                            .iter()
                            .map(|o| u16::from(b.0.abs_diff(o.0)) + u16::from(b.1.abs_diff(o.1)))
                            .min()
                            .unwrap_or(u16::MAX);
                        b_min.cmp(&a_min)
                    });
                }
                if let Some(&(x, y)) = free_cells.first() {
                    mgr.placement_budget -= FURNITURE_COST;
                    mgr.layout.furniture.push(FurniturePlacement {
                        asset_id: asset_id.clone(),
                        x,
                        y,
                        zone: zone_kind,
                        placed_at_tick: tick,
                    });
                    mgr.construction_fx.push(ConstructionFx {
                        x,
                        y,
                        expires_at_tick: tick + 3,
                    });
                    new_events.push(SimEvent {
                        id: Uuid::new_v4(),
                        tick,
                        timestamp: Utc::now(),
                        category: "furniture_placed".to_string(),
                        severity: "info".to_string(),
                        message: format!(
                            "Floor {} placed {} at ({}, {})",
                            floor.name, asset_id, x, y
                        ),
                        refs: vec![floor.id.to_string()],
                    });
                }
            }
        }
    }

    // Tick pipeline
    if let Some(ref mut pipeline) = mgr.pipeline {
        tick_pipeline(pipeline, tick, &floor.name, floor.id, new_events);
    }

    // Overloaded crunch mode: remove break room items, add desks
    if floor.stage == FloorStage::Overloaded {
        let had_break = mgr
            .layout
            .furniture
            .iter()
            .any(|f| f.zone == ZoneKind::BreakRoom);
        mgr.layout
            .furniture
            .retain(|f| f.zone != ZoneKind::BreakRoom);
        if had_break && (mgr.layout.furniture.len() as u8) < mgr.layout.max_slots {
            // Add a desk in the work area
            if let Some(work_zone) = mgr
                .layout
                .zones
                .iter()
                .find(|z| z.kind == ZoneKind::WorkArea)
            {
                let occupied: std::collections::HashSet<(u8, u8)> =
                    mgr.layout.furniture.iter().map(|f| (f.x, f.y)).collect();
                let free: Vec<(u8, u8)> = cells_in_zone(work_zone)
                    .into_iter()
                    .filter(|c| !occupied.contains(c))
                    .collect();
                if let Some(&(x, y)) = free.first() {
                    mgr.layout.furniture.push(FurniturePlacement {
                        asset_id: "prop_lpc_desk_ornate".to_string(),
                        x,
                        y,
                        zone: ZoneKind::WorkArea,
                        placed_at_tick: tick,
                    });
                }
            }
        }
    }
}

fn tick_pipeline(
    pipeline: &mut Pipeline,
    tick: u64,
    floor_name: &str,
    floor_id: Uuid,
    new_events: &mut Vec<SimEvent>,
) {
    if pipeline.stages.is_empty() {
        return;
    }
    pipeline.progress_ticks += 1;
    let current = &pipeline.stages[pipeline.current_stage_idx];
    if pipeline.progress_ticks >= current.duration_ticks {
        pipeline.progress_ticks = 0;
        pipeline.current_stage_idx += 1;
        if pipeline.current_stage_idx >= pipeline.stages.len() {
            pipeline.current_stage_idx = 0;
            pipeline.total_completions += 1;
            new_events.push(SimEvent {
                id: Uuid::new_v4(),
                tick,
                timestamp: Utc::now(),
                category: "pipeline_completed".to_string(),
                severity: "info".to_string(),
                message: format!(
                    "Floor {} pipeline cycle #{} completed",
                    floor_name, pipeline.total_completions
                ),
                refs: vec![floor_id.to_string()],
            });
        }
    }
}

fn create_pipeline(kind: PipelineKind, work_zone: &OfficeZone) -> Pipeline {
    let stages_def: Vec<(&str, &str)> = match kind {
        PipelineKind::CiCd => vec![
            ("push", "prop_push_desk"),
            ("test", "prop_test_desk"),
            ("review", "prop_review_desk"),
            ("deploy", "prop_commit_desk"),
        ],
        PipelineKind::MailRoom => vec![
            ("receive", "prop_lpc_mailboxes"),
            ("sort", "prop_lpc_bins"),
            ("distribute", "prop_lpc_desk_ornate"),
        ],
        PipelineKind::FoodService => vec![
            ("prep", "prop_lpc_coffee_maker"),
            ("cook", "prop_lpc_water_cooler"),
            ("serve", "prop_lpc_card_table"),
        ],
    };
    let base_x = work_zone.x_min;
    let base_y = work_zone.y_min;
    let stages = stages_def
        .iter()
        .enumerate()
        .map(|(i, (name, asset))| PipelineStage {
            name: name.to_string(),
            asset_id: asset.to_string(),
            x: base_x.saturating_add(i as u8 * 2).min(work_zone.x_max),
            y: base_y,
            duration_ticks: 5,
        })
        .collect();
    Pipeline {
        kind,
        stages,
        current_stage_idx: 0,
        progress_ticks: 0,
        total_completions: 0,
    }
}

pub fn operator_review(state: &mut TowerState) {
    if state.tick < state.operator.next_review_tick {
        return;
    }
    state.operator.next_review_tick = state.tick + OPERATOR_REVIEW_INTERVAL;
    state.operator.budget += 10;

    for floor in &state.floors {
        // Under-furnished for stage?
        let max_slots = max_slots_for_stage(floor.floor_type, floor.stage);
        let current_count = floor.manager.layout.furniture.len() as u8;
        let has_upgrade = floor
            .manager
            .scripts
            .iter()
            .any(|s| matches!(s.config, ScriptConfig::FurnitureUpgrade { .. }));
        if current_count < max_slots / 2 && !has_upgrade {
            state.operator.pending_directives.push(OperatorDirective {
                target_floor_id: floor.id,
                action: DirectiveAction::UpgradeFurniture,
                issued_at_tick: state.tick,
            });
        }

        // Commercial at Growing+ without pipeline?
        if floor.floor_type == FloorType::Commercial
            && floor.stage >= FloorStage::Growing
            && floor.manager.pipeline.is_none()
        {
            state.operator.pending_directives.push(OperatorDirective {
                target_floor_id: floor.id,
                action: DirectiveAction::InstallPipeline {
                    kind: PipelineKind::CiCd,
                },
                issued_at_tick: state.tick,
            });
        }

        // Service floor → bias toward BreakRoom
        if floor.floor_type == FloorType::Service {
            let has_bias = floor
                .manager
                .scripts
                .iter()
                .any(|s| matches!(s.config, ScriptConfig::LayoutBias { .. }));
            if !has_bias {
                state.operator.pending_directives.push(OperatorDirective {
                    target_floor_id: floor.id,
                    action: DirectiveAction::SetLayoutBias {
                        work_pct: 0.3,
                        break_pct: 0.5,
                    },
                    issued_at_tick: state.tick,
                });
            }
        }
    }
}

pub fn apply_pending_directives(state: &mut TowerState) {
    let directives = std::mem::take(&mut state.operator.pending_directives);
    let tick = state.tick;
    for directive in directives {
        let Some(floor) = state
            .floors
            .iter_mut()
            .find(|f| f.id == directive.target_floor_id)
        else {
            continue;
        };
        match directive.action {
            DirectiveAction::SetLayoutBias {
                work_pct,
                break_pct,
            } => {
                floor.manager.scripts.push(ManagerScript {
                    script_id: format!("layout_bias_{}", tick),
                    installed_at_tick: tick,
                    config: ScriptConfig::LayoutBias {
                        work_pct,
                        break_pct,
                    },
                });
            }
            DirectiveAction::InstallScript { script_id, config } => {
                floor.manager.scripts.push(ManagerScript {
                    script_id,
                    installed_at_tick: tick,
                    config,
                });
            }
            DirectiveAction::InstallPipeline { kind } => {
                if floor.manager.pipeline.is_none() {
                    let work_zone = floor
                        .manager
                        .layout
                        .zones
                        .iter()
                        .find(|z| z.kind == ZoneKind::WorkArea)
                        .cloned()
                        .unwrap_or(OfficeZone {
                            kind: ZoneKind::WorkArea,
                            x_min: 1,
                            y_min: 1,
                            x_max: floor.tilemap.width.saturating_sub(2),
                            y_max: floor.tilemap.height / 2,
                        });
                    floor.manager.pipeline = Some(create_pipeline(kind, &work_zone));
                }
            }
            DirectiveAction::UpgradeFurniture => {
                let has_upgrade = floor
                    .manager
                    .scripts
                    .iter()
                    .any(|s| matches!(s.config, ScriptConfig::FurnitureUpgrade { .. }));
                if !has_upgrade {
                    floor.manager.scripts.push(ManagerScript {
                        script_id: format!("furniture_upgrade_{}", tick),
                        installed_at_tick: tick,
                        config: ScriptConfig::FurnitureUpgrade { tier: 1 },
                    });
                }
            }
        }
    }
}

fn goal_label(goal: AgentGoal) -> &'static str {
    match goal {
        AgentGoal::Work => "work",
        AgentGoal::Eat => "eat",
        AgentGoal::Rest => "rest",
        AgentGoal::Socialize => "socialize",
        AgentGoal::Relocate => "relocate",
    }
}

fn zone_label(kind: ZoneKind) -> &'static str {
    match kind {
        ZoneKind::WorkArea => "work_area",
        ZoneKind::BreakRoom => "break_room",
        ZoneKind::Reception => "reception",
        ZoneKind::Utility => "utility",
        ZoneKind::Corridor => "corridor",
    }
}

fn pipeline_kind_label(kind: PipelineKind) -> &'static str {
    match kind {
        PipelineKind::CiCd => "ci_cd",
        PipelineKind::MailRoom => "mail_room",
        PipelineKind::FoodService => "food_service",
    }
}

fn lease_status_label(status: LeaseStatus) -> &'static str {
    match status {
        LeaseStatus::Active => "active",
        LeaseStatus::Delinquent => "delinquent",
        LeaseStatus::Evicted => "evicted",
    }
}

fn script_config_token(config: &ScriptConfig) -> String {
    match config {
        ScriptConfig::LayoutBias {
            work_pct,
            break_pct,
        } => format!(
            "layout_bias:{:08x}:{:08x}",
            work_pct.to_bits(),
            break_pct.to_bits()
        ),
        ScriptConfig::PipelineInstall { pipeline_kind } => {
            format!("pipeline_install:{}", pipeline_kind_label(*pipeline_kind))
        }
        ScriptConfig::FurnitureUpgrade { tier } => format!("furniture_upgrade:{tier}"),
    }
}

fn directive_token(action: &DirectiveAction) -> String {
    match action {
        DirectiveAction::SetLayoutBias {
            work_pct,
            break_pct,
        } => format!(
            "set_layout_bias:{:08x}:{:08x}",
            work_pct.to_bits(),
            break_pct.to_bits()
        ),
        DirectiveAction::InstallScript { script_id, config } => {
            format!("install_script:{script_id}:{}", script_config_token(config))
        }
        DirectiveAction::InstallPipeline { kind } => {
            format!("install_pipeline:{}", pipeline_kind_label(*kind))
        }
        DirectiveAction::UpgradeFurniture => "upgrade_furniture".to_string(),
    }
}

fn fnv1a64(bytes: &[u8], offset: u64) -> u64 {
    let mut hash = offset;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn floor_index(state: &TowerState, id: Uuid) -> String {
    state
        .floors
        .iter()
        .position(|floor| floor.id == id)
        .map(|index| index.to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Canonical, order-stable encoding of the seeded simulation state.
///
/// Random UUIDs and wall-clock timestamps are omitted. Entity ids are replaced
/// with creation-order indexes. Alert order is sorted because dedupe stores
/// them in a `HashMap`, and that order does not feed later ticks.
fn canonical_state(state: &TowerState) -> String {
    let mut out = String::new();
    out.push_str(&format!("tower={}\n", state.tower_id));
    out.push_str(&format!("tick={}\n", state.tick));
    out.push_str(&format!(
        "next_floor_build_tick={}\n",
        state.next_floor_build_tick
    ));
    out.push_str(&format!("expansion_count={}\n", state.expansion_count));
    out.push_str(&format!(
        "economy={},{},{},{}\n",
        state.economy.wages_paid,
        state.economy.rent_collected,
        state.economy.service_spend,
        state.economy.unmet_need_penalty
    ));
    out.push_str(&format!(
        "operator={},{},{}\n",
        state.operator.budget,
        state.operator.next_review_tick,
        state
            .operator
            .pending_directives
            .iter()
            .map(|directive| format!(
                "{}:{}@{}",
                floor_index(state, directive.target_floor_id),
                directive_token(&directive.action),
                directive.issued_at_tick
            ))
            .collect::<Vec<_>>()
            .join(";")
    ));

    let mut alerts: Vec<String> = state
        .alerts
        .iter()
        .map(|alert| format!("{}|{}|{}", alert.severity, alert.category, alert.message))
        .collect();
    alerts.sort();
    out.push_str("alerts=");
    out.push_str(&alerts.join(";"));
    out.push('\n');

    let mut policies: Vec<String> = state
        .policies
        .iter()
        .map(|policy| {
            format!(
                "{}:{:08x}:{}:{}",
                policy.key,
                policy.value.to_bits(),
                policy.created_at_tick,
                policy
                    .expires_at_tick
                    .map(|tick| tick.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )
        })
        .collect();
    policies.sort();
    out.push_str("policies=");
    out.push_str(&policies.join(";"));
    out.push('\n');

    let mut signals: Vec<String> = state
        .active_signals
        .iter()
        .map(|signal| {
            format!(
                "{}|{}|{:08x}|{}|{}",
                signal.signal_type,
                signal.target_zone,
                signal.intensity.to_bits(),
                signal.created_at_tick,
                signal.expires_at_tick
            )
        })
        .collect();
    signals.sort();
    out.push_str("signals=");
    out.push_str(&signals.join(";"));
    out.push('\n');

    for (index, agent) in state.agents.iter().enumerate() {
        let lease = agent
            .lease_id
            .and_then(|lease_id| state.leases.get(&lease_id));
        let lease_token = match lease {
            Some(lease) => format!(
                "{}:{}:{}",
                lease_status_label(lease.status),
                lease.rent,
                lease.due_tick
            ),
            None => "none".to_string(),
        };
        out.push_str(&format!(
            "agent={index},{},{},{},{},{},{},{},{},{},{},{}\n",
            agent.role,
            agent.credits,
            agent.needs.hunger,
            agent.needs.energy,
            agent.needs.social,
            agent.needs.comfort,
            agent.mood,
            goal_label(agent.current_goal),
            floor_index(state, agent.location_floor_id),
            agent.debt_ticks,
            lease_token
        ));
    }

    for (index, floor) in state.floors.iter().enumerate() {
        let furniture = floor
            .manager
            .layout
            .furniture
            .iter()
            .map(|item| {
                format!(
                    "{}@{},{}:{}:{}",
                    item.asset_id,
                    item.x,
                    item.y,
                    zone_label(item.zone),
                    item.placed_at_tick
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let zones = floor
            .manager
            .layout
            .zones
            .iter()
            .map(|zone| {
                format!(
                    "{}:{},{}-{},{}",
                    zone_label(zone.kind),
                    zone.x_min,
                    zone.y_min,
                    zone.x_max,
                    zone.y_max
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let scripts = floor
            .manager
            .scripts
            .iter()
            .map(|script| {
                format!(
                    "{}@{}:{}",
                    script.script_id,
                    script.installed_at_tick,
                    script_config_token(&script.config)
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        let pipeline = floor
            .manager
            .pipeline
            .as_ref()
            .map(|pipeline| {
                let stages = pipeline
                    .stages
                    .iter()
                    .map(|stage| {
                        format!(
                            "{}:{}@{},{}:{}",
                            stage.name, stage.asset_id, stage.x, stage.y, stage.duration_ticks
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{}:{}:{}:{}:[{stages}]",
                    pipeline_kind_label(pipeline.kind),
                    pipeline.current_stage_idx,
                    pipeline.progress_ticks,
                    pipeline.total_completions
                )
            })
            .unwrap_or_else(|| "-".to_string());
        let fx = floor
            .manager
            .construction_fx
            .iter()
            .map(|fx| format!("{},{}@{}", fx.x, fx.y, fx.expires_at_tick))
            .collect::<Vec<_>>()
            .join(";");
        out.push_str(&format!(
            "floor={index},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            floor
                .name
                .chars()
                .map(|ch| if ch == ',' || ch == '\n' { ' ' } else { ch })
                .collect::<String>(),
            floor_type_ref_label(floor.floor_type),
            floor.template_id,
            floor_stage_label(floor.stage),
            floor.stage_score,
            floor.base_capacity,
            floor.base_upkeep,
            floor.base_rent_curve,
            floor.capacity,
            floor.occupancy,
            floor.upkeep,
            floor.rent_curve,
            floor.manager.age_ticks,
            floor.manager.placement_budget,
            floor.manager.layout.max_slots,
            floor.tilemap.width,
            floor.tilemap.height,
            floor.tilemap.tiles.join(",")
        ));
        out.push_str(&format!(
            "floor_layout={index}|{furniture}|{zones}|{scripts}|{pipeline}|{fx}\n"
        ));
    }

    out
}

/// Stable fingerprint of the seed-driven simulation state.
///
/// Two towers initialized with the same seed and advanced the same number of
/// ticks produce the same fingerprint. The digest covers needs, economy,
/// goals, leases, floor progression, furniture, scripts, and pipelines. It
/// leaves out random UUIDs and `Utc::now()` timestamps, which do not feed
/// later ticks.
pub fn state_fingerprint(state: &TowerState) -> String {
    let canonical = canonical_state(state);
    let bytes = canonical.as_bytes();
    let high = fnv1a64(bytes, 0xcbf2_9ce4_8422_2325);
    let low = fnv1a64(bytes, 0x8422_2325_cbf2_9ce4);
    format!("{high:016x}{low:016x}")
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
        if let Some(lease_id) = first.lease_id
            && let Some(lease) = tower.leases.get_mut(&lease_id)
        {
            lease.due_tick = 1;
            lease.status = LeaseStatus::Delinquent;
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
        tower.floors[0].template_id.clear();
        tower.floors[0].base_capacity = 0;
        tower.floors[0].base_upkeep = 0;
        tower.floors[0].base_rent_curve = 0;
        ensure_floor_tilemaps(&mut tower);
        let map = &tower.floors[0].tilemap;
        assert!(map.width > 0);
        assert!(map.height > 0);
        assert_eq!(map.tiles.len(), map.width as usize * map.height as usize);
        assert!(map.tiles.iter().any(|id| id.starts_with("tile_wall_")));
        assert!(!tower.floors[0].template_id.is_empty());
        assert!(tower.floors[0].base_capacity > 0);
        assert!(tower.floors[0].base_upkeep > 0);
        assert!(tower.floors[0].base_rent_curve > 0);
    }

    #[test]
    fn floor_progression_tracks_load_and_emits_transition_event() {
        let mut tower = initialize_tower(17);
        let work_hub_id = tower
            .floors
            .iter()
            .find(|f| f.floor_type == FloorType::Commercial)
            .map(|f| f.id)
            .expect("commercial floor exists");
        for agent in &mut tower.agents {
            agent.location_floor_id = work_hub_id;
            agent.mood = 95;
            agent.needs = Needs {
                hunger: 95,
                energy: 95,
                social: 95,
                comfort: 95,
            };
        }
        let tick = run_tick(&mut tower);
        let work_hub = tower
            .floors
            .iter()
            .find(|f| f.id == work_hub_id)
            .expect("commercial floor exists");
        assert_eq!(work_hub.stage, FloorStage::Overloaded);
        assert!(work_hub.stage_score >= 90);
        assert_eq!(work_hub.template_id, "commercial_overloaded_v1");
        assert!(tick.diff.new_events.iter().any(|e| {
            e.category == "floor_progression"
                && e.message.contains("Work Hub")
                && e.message.contains("overloaded")
        }));
    }

    #[test]
    fn floor_template_catalog_covers_all_floor_type_stage_pairs() {
        let floor_types = [
            FloorType::Residential,
            FloorType::Commercial,
            FloorType::Service,
            FloorType::Utility,
        ];
        let stages = [
            FloorStage::Seed,
            FloorStage::Growing,
            FloorStage::Mature,
            FloorStage::Overloaded,
        ];
        for floor_type in floor_types {
            for stage in stages {
                let spec = find_floor_template(floor_type, stage)
                    .unwrap_or_else(|| panic!("missing template for {:?}/{:?}", floor_type, stage));
                assert!(!spec.template_id.trim().is_empty());
                assert!(spec.capacity_multiplier_pct > 0);
                assert!(spec.upkeep_multiplier_pct > 0);
            }
        }
    }

    #[test]
    fn floor_template_settings_load_expected_thresholds() {
        let settings = floor_template_settings();
        assert_eq!(settings.stage_thresholds.growing_min_score, 35);
        assert_eq!(settings.stage_thresholds.mature_min_score, 65);
        assert_eq!(settings.stage_thresholds.overloaded_min_score, 90);
        assert_eq!(settings.expansion.max_floors, 8);
        assert_eq!(settings.expansion.build_cooldown_ticks, 120);
        assert_eq!(settings.expansion.min_avg_stage_score, 30);
    }

    #[test]
    fn overloaded_floors_trigger_construction_when_cooldown_elapsed() {
        let mut tower = initialize_tower(23);
        tower.next_floor_build_tick = 0;
        let work_hub_id = tower
            .floors
            .iter()
            .find(|f| f.floor_type == FloorType::Commercial)
            .map(|f| f.id)
            .expect("commercial floor exists");
        for agent in &mut tower.agents {
            agent.location_floor_id = work_hub_id;
            agent.mood = 95;
            agent.needs = Needs {
                hunger: 95,
                energy: 95,
                social: 95,
                comfort: 95,
            };
        }
        let initial_floor_count = tower.floors.len();
        let tick = run_tick(&mut tower);
        assert_eq!(tower.floors.len(), initial_floor_count + 1);
        assert_eq!(tower.expansion_count, 1);
        assert!(tower.next_floor_build_tick > tower.tick);
        assert!(
            tick.diff
                .new_events
                .iter()
                .any(|e| e.category == "floor_construction")
        );
    }

    #[test]
    fn floor_manager_places_furniture_over_time() {
        let mut tower = initialize_tower(42);
        // Run enough ticks for budget to accumulate (FURNITURE_COST = 5)
        for _ in 0..10 {
            run_tick(&mut tower);
        }
        let commercial = tower
            .floors
            .iter()
            .find(|f| f.floor_type == FloorType::Commercial)
            .unwrap();
        assert!(
            !commercial.manager.layout.furniture.is_empty(),
            "manager should have placed at least one furniture after 10 ticks"
        );
        assert!(commercial.manager.age_ticks >= 10);
    }

    #[test]
    fn floor_manager_respects_max_slots() {
        let mut tower = initialize_tower(42);
        for tick_num in 0..80 {
            run_tick(&mut tower);
            for floor in &tower.floors {
                let max = max_slots_for_stage(floor.floor_type, floor.stage);
                assert!(
                    floor.manager.layout.furniture.len() <= max as usize,
                    "tick {}: floor {} ({:?}) has {} furniture but max_slots is {} at stage {:?}",
                    tick_num + 1,
                    floor.name,
                    floor.floor_type,
                    floor.manager.layout.furniture.len(),
                    max,
                    floor.stage,
                );
            }
        }
    }

    #[test]
    fn zones_computed_correctly() {
        let tilemap = build_floor_tilemap(FloorType::Commercial);
        let walkable = walkable_cells_for_tilemap(&tilemap);
        let zones = compute_zones(&tilemap, &walkable, None);
        assert!(
            zones.len() >= 3,
            "should have at least WorkArea, BreakRoom, Reception"
        );
        assert!(
            zones.iter().any(|z| z.kind == ZoneKind::WorkArea),
            "missing WorkArea zone"
        );
        assert!(
            zones.iter().any(|z| z.kind == ZoneKind::BreakRoom),
            "missing BreakRoom zone"
        );
        assert!(
            zones.iter().any(|z| z.kind == ZoneKind::Reception),
            "missing Reception zone"
        );
    }

    #[test]
    fn operator_issues_directives() {
        let mut tower = initialize_tower(42);
        // Force review by setting next_review_tick to 0
        tower.operator.next_review_tick = 0;
        tower.tick = 1;
        operator_review(&mut tower);
        // Operator should have issued some directives
        assert!(
            !tower.operator.pending_directives.is_empty(),
            "operator should issue directives on review"
        );
    }

    #[test]
    fn directives_consumed_by_managers() {
        let mut tower = initialize_tower(42);
        let floor_id = tower.floors[0].id;
        tower.operator.pending_directives.push(OperatorDirective {
            target_floor_id: floor_id,
            action: DirectiveAction::UpgradeFurniture,
            issued_at_tick: 1,
        });
        apply_pending_directives(&mut tower);
        assert!(
            tower.operator.pending_directives.is_empty(),
            "directives should be consumed"
        );
        assert!(
            tower.floors[0]
                .manager
                .scripts
                .iter()
                .any(|s| matches!(s.config, ScriptConfig::FurnitureUpgrade { .. })),
            "upgrade script should be installed"
        );
    }

    #[test]
    fn scripts_modify_layout_behavior() {
        let mut tower = initialize_tower(42);
        let floor_id = tower
            .floors
            .iter()
            .find(|f| f.floor_type == FloorType::Commercial)
            .unwrap()
            .id;
        // Install upgrade script
        tower.operator.pending_directives.push(OperatorDirective {
            target_floor_id: floor_id,
            action: DirectiveAction::InstallScript {
                script_id: "test_upgrade".to_string(),
                config: ScriptConfig::FurnitureUpgrade { tier: 1 },
            },
            issued_at_tick: 1,
        });
        apply_pending_directives(&mut tower);
        // Now run ticks so furniture gets placed with upgrade palette
        for _ in 0..20 {
            run_tick(&mut tower);
        }
        let floor = tower.floors.iter().find(|f| f.id == floor_id).unwrap();
        let has_upgrade_item = floor
            .manager
            .layout
            .furniture
            .iter()
            .any(|f| UPGRADE_PALETTE.contains(&f.asset_id.as_str()));
        assert!(has_upgrade_item, "upgraded palette items should be placed");
    }

    #[test]
    fn pipeline_advances_stages_and_completes() {
        let zone = OfficeZone {
            kind: ZoneKind::WorkArea,
            x_min: 1,
            y_min: 1,
            x_max: 10,
            y_max: 5,
        };
        let mut pipeline = create_pipeline(PipelineKind::CiCd, &zone);
        assert_eq!(pipeline.stages.len(), 4);
        assert_eq!(pipeline.current_stage_idx, 0);
        assert_eq!(pipeline.total_completions, 0);

        let floor_id = Uuid::new_v4();
        let mut events = vec![];
        // Advance through all stages (4 stages x 5 ticks each = 20 ticks)
        for tick in 1..=20 {
            tick_pipeline(&mut pipeline, tick, "test", floor_id, &mut events);
        }
        assert_eq!(
            pipeline.total_completions, 1,
            "pipeline should complete one cycle"
        );
        assert!(
            events.iter().any(|e| e.category == "pipeline_completed"),
            "should emit pipeline_completed event"
        );
    }

    #[test]
    fn legacy_floors_still_render_without_manager_furniture() {
        // A floor with empty manager should use the legacy prop path
        let tower = initialize_tower(42);
        // At tick 0, no furniture placed yet - legacy path should work
        // This test just ensures the tower initializes cleanly
        assert!(
            tower
                .floors
                .iter()
                .all(|f| f.manager.layout.furniture.is_empty())
        );
    }

    #[test]
    fn equal_scores_keep_the_earlier_goal() {
        let agent = Agent {
            id: Uuid::new_v4(),
            role: "worker".to_string(),
            credits: 200,
            needs: Needs {
                hunger: 63,
                energy: 63,
                social: 63,
                comfort: 63,
            },
            mood: 70,
            current_goal: AgentGoal::Work,
            location_floor_id: Uuid::new_v4(),
            lease_id: None,
            debt_ticks: 0,
        };
        let trace = choose_goal(&agent);
        assert_eq!(trace.priority_scores.get("eat"), Some(&37));
        assert_eq!(trace.priority_scores.get("work"), Some(&37));
        assert_eq!(trace.decision, AgentGoal::Eat);
        assert_eq!(choose_goal(&agent).decision, trace.decision);
    }

    #[test]
    fn expansion_tie_break_prefers_the_higher_floor_rank() {
        let mut tower = initialize_tower(1);
        for floor in &mut tower.floors {
            floor.stage = FloorStage::Overloaded;
        }
        assert_eq!(next_expansion_floor_type(&tower), FloorType::Utility);
        assert_eq!(
            next_expansion_floor_type(&tower),
            next_expansion_floor_type(&tower)
        );
    }

    fn simulate(seed: u64, ticks: u32) -> TowerState {
        let mut state = initialize_tower(seed);
        for _ in 0..ticks {
            run_tick(&mut state);
        }
        state
    }

    fn assert_same_seed(seed: u64, ticks: u32) {
        let left = simulate(seed, ticks);
        let right = simulate(seed, ticks);
        let left_state = canonical_state(&left);
        let right_state = canonical_state(&right);
        if left_state != right_state {
            let mismatch = left_state
                .lines()
                .zip(right_state.lines())
                .enumerate()
                .find(|(_, (lhs, rhs))| lhs != rhs);
            panic!("seed {seed} diverged after {ticks} ticks at {mismatch:?}");
        }
        assert_eq!(state_fingerprint(&left), state_fingerprint(&right));
        assert_eq!(
            state_fingerprint(&left),
            state_fingerprint(&left),
            "fingerprint must be a pure function of state"
        );
        check_invariants(&left, u64::from(ticks)).unwrap();
    }

    #[test]
    fn same_seed_matches_after_n_ticks() {
        for (seed, ticks) in [
            (0u64, 1u32),
            (1, 100),
            (7, 25),
            (42, 100),
            (99, 200),
            (u64::MAX, 40),
        ] {
            assert_same_seed(seed, ticks);
        }
    }

    #[test]
    fn pinned_state_fingerprints() {
        let cases = [
            (1u64, 50u32, "9e734df1a4c482fe240d75b558ebcb8b"),
            (42, 100, "9bae7cbb50d2b12a160a60a9514e7733"),
            (7, 25, "0dea91f4254303582fdfcc926951ec29"),
            (99, 200, "592a3ff115e4ae3ca7bb7e42b047e7b9"),
        ];
        for (seed, ticks, expected) in cases {
            let actual = state_fingerprint(&simulate(seed, ticks));
            assert_eq!(actual, expected, "seed {seed} after {ticks} ticks");
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let left = state_fingerprint(&simulate(1, 30));
        let right = state_fingerprint(&simulate(2, 30));
        assert_ne!(left, right);
    }

    fn check_invariants(state: &TowerState, expected_tick: u64) -> Result<(), String> {
        if state.agents.len() != MAX_AGENTS {
            return Err(format!("agent count {}", state.agents.len()));
        }
        if state.tick != expected_tick {
            return Err(format!("tick {} != {expected_tick}", state.tick));
        }
        if !(3..=8).contains(&state.floors.len()) {
            return Err(format!("floor count {}", state.floors.len()));
        }
        if state.economy.wages_paid < 0
            || state.economy.rent_collected < 0
            || state.economy.service_spend < 0
            || state.economy.unmet_need_penalty < 0
        {
            return Err(format!("negative economy {:?}", state.economy));
        }

        let mut agent_ids = std::collections::HashSet::new();
        let mut lease_ids = std::collections::HashSet::new();
        let floor_ids: std::collections::HashSet<Uuid> =
            state.floors.iter().map(|floor| floor.id).collect();
        for agent in &state.agents {
            if !agent_ids.insert(agent.id) {
                return Err(format!("duplicate agent {}", agent.id));
            }
            if !floor_ids.contains(&agent.location_floor_id) {
                return Err(format!("agent {} is on an unknown floor", agent.id));
            }
            if agent.needs.hunger > 100
                || agent.needs.energy > 100
                || agent.needs.social > 100
                || agent.needs.comfort > 100
            {
                return Err(format!("needs out of range for {}", agent.id));
            }
            if !(0..=100).contains(&agent.mood) {
                return Err(format!("mood {} for {}", agent.mood, agent.id));
            }
            if agent.credits < 0 {
                return Err(format!(
                    "negative credits {} for {}",
                    agent.credits, agent.id
                ));
            }
            if agent.debt_ticks > 3 {
                return Err(format!("debt ticks {} for {}", agent.debt_ticks, agent.id));
            }
            let Some(lease_id) = agent.lease_id else {
                return Err(format!("agent {} has no lease", agent.id));
            };
            if !lease_ids.insert(lease_id) {
                return Err(format!("duplicate lease {lease_id}"));
            }
            let Some(lease) = state.leases.get(&lease_id) else {
                return Err(format!("missing lease {lease_id}"));
            };
            if lease.agent_id != agent.id {
                return Err(format!("lease {lease_id} points at the wrong agent"));
            }
            if lease.rent <= 0 {
                return Err(format!("non-positive rent on {lease_id}"));
            }
            if lease.status == LeaseStatus::Evicted && agent.debt_ticks < 3 {
                return Err(format!(
                    "evicted agent {} has debt {}",
                    agent.id, agent.debt_ticks
                ));
            }
        }

        for floor in &state.floors {
            let occupants = state
                .agents
                .iter()
                .filter(|agent| agent.location_floor_id == floor.id)
                .count() as u16;
            if floor.occupancy != occupants {
                return Err(format!(
                    "floor {} occupancy {} != {occupants}",
                    floor.name, floor.occupancy
                ));
            }
            if floor.capacity < 1 || floor.upkeep < 1 || floor.rent_curve < 1 {
                return Err(format!(
                    "floor {} has a non-positive economy stat",
                    floor.name
                ));
            }
            if floor.stage_score > 100 {
                return Err(format!(
                    "stage score {} on {}",
                    floor.stage_score, floor.name
                ));
            }
            if floor.stage != stage_from_score(floor.stage_score) {
                return Err(format!(
                    "floor {} stage {:?} does not match score {}",
                    floor.name, floor.stage, floor.stage_score
                ));
            }
            if floor.template_id != default_floor_template_id(floor.floor_type, floor.stage) {
                return Err(format!(
                    "floor {} template {} != expected",
                    floor.name, floor.template_id
                ));
            }
            if floor.template_id.trim().is_empty() {
                return Err(format!("floor {} has an empty template", floor.name));
            }
            let max_slots = usize::from(max_slots_for_stage(floor.floor_type, floor.stage));
            if floor.manager.layout.furniture.len() > max_slots {
                return Err(format!(
                    "floor {} has {} furniture, max {max_slots}",
                    floor.name,
                    floor.manager.layout.furniture.len()
                ));
            }
            if floor.manager.placement_budget > 30 {
                return Err(format!(
                    "floor {} budget {}",
                    floor.name, floor.manager.placement_budget
                ));
            }
            if floor.manager.age_ticks > state.tick {
                return Err(format!(
                    "floor {} age {} exceeds tick {}",
                    floor.name, floor.manager.age_ticks, state.tick
                ));
            }
            for item in &floor.manager.layout.furniture {
                if item.x >= floor.tilemap.width || item.y >= floor.tilemap.height {
                    return Err(format!(
                        "furniture on {} is outside the tilemap at {},{}",
                        floor.name, item.x, item.y
                    ));
                }
            }
            if let Some(pipeline) = &floor.manager.pipeline {
                if pipeline.stages.is_empty() {
                    return Err(format!("floor {} has an empty pipeline", floor.name));
                }
                if pipeline.current_stage_idx >= pipeline.stages.len() {
                    return Err(format!(
                        "floor {} pipeline index is past the end",
                        floor.name
                    ));
                }
                let duration = pipeline.stages[pipeline.current_stage_idx].duration_ticks;
                if pipeline.progress_ticks >= duration {
                    return Err(format!(
                        "floor {} pipeline progress {} >= {duration}",
                        floor.name, pipeline.progress_ticks
                    ));
                }
            }
            let tile_len = usize::from(floor.tilemap.width) * usize::from(floor.tilemap.height);
            if floor.tilemap.tiles.len() != tile_len || tile_len == 0 {
                return Err(format!("floor {} tilemap has the wrong length", floor.name));
            }
        }

        let metrics = compute_metrics(state);
        if !metrics.tower_health.is_finite()
            || !(0.0..=100.0).contains(&metrics.tower_health)
            || !metrics.avg_need_satisfaction.is_finite()
            || !metrics.liquidity.is_finite()
            || !metrics.occupancy.is_finite()
        {
            return Err(format!("non-finite metrics at tick {}", state.tick));
        }
        Ok(())
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn seeded_runs_match_and_hold_invariants(seed: u64, ticks in 0u32..20) {
            let left = simulate(seed, ticks);
            let right = simulate(seed, ticks);
            prop_assert_eq!(state_fingerprint(&left), state_fingerprint(&right));
            check_invariants(&left, u64::from(ticks)).unwrap();
            check_invariants(&right, u64::from(ticks)).unwrap();
        }
    }
}
