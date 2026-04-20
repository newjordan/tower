use crate::sim::{
    DepartmentHealth, DepartmentStateUpdate, DeskActivityUpdate, FloorPrestige, RankingEntry,
    SimEvent,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub const CODING_DEPARTMENT: &str = "coding";
pub const ALLOWED_EVENT_TYPES: [&str; 7] = [
    "task_started",
    "task_completed",
    "task_failed",
    "git_pull",
    "git_commit",
    "git_push",
    "test_run",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawEvent {
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub tower_id: String,
    pub floor_id: String,
    pub department_id: String,
    pub event_type: String,
    pub task_id: Option<String>,
    pub agent_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct OpsState {
    pub desk_activity: HashMap<String, DeskActivityUpdate>,
    pub department_state: HashMap<String, DepartmentStateUpdate>,
    pub floor_usage: HashMap<String, FloorUsage>,
}

#[derive(Debug, Clone)]
pub struct FloorUsage {
    pub tower_id: String,
    pub floor_id: String,
    pub successful_runs: u64,
    pub active_runtime_seconds: u64,
    pub current_streak_days: u32,
    pub last_active_date: NaiveDate,
    pub last_active_at: DateTime<Utc>,
    pub failures_today_date: NaiveDate,
    pub failures_today: u32,
    pub recovery_success_count: u32,
}

impl FloorUsage {
    fn new(tower_id: String, floor_id: String, now: DateTime<Utc>) -> Self {
        Self {
            tower_id,
            floor_id,
            successful_runs: 0,
            active_runtime_seconds: 0,
            current_streak_days: 1,
            last_active_date: now.date_naive(),
            last_active_at: now,
            failures_today_date: now.date_naive(),
            failures_today: 0,
            recovery_success_count: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessOutcome {
    pub desk_update: Option<DeskActivityUpdate>,
    pub department_update: DepartmentStateUpdate,
    pub prestige: FloorPrestige,
    pub sim_event: SimEvent,
}

pub fn process_event(
    state: &mut OpsState,
    event: OpenClawEvent,
    now: DateTime<Utc>,
) -> Result<ProcessOutcome, String> {
    if !ALLOWED_EVENT_TYPES.contains(&event.event_type.as_str()) {
        return Err(format!("unsupported event_type: {}", event.event_type));
    }

    let usage_key = floor_usage_key(&event.tower_id, &event.floor_id);
    let usage = state
        .floor_usage
        .entry(usage_key)
        .or_insert_with(|| FloorUsage::new(event.tower_id.clone(), event.floor_id.clone(), now));
    refresh_streak(usage, now);
    usage.active_runtime_seconds = usage.active_runtime_seconds.saturating_add(60);
    usage.last_active_at = now;

    if event.event_type == "task_completed" {
        usage.successful_runs = usage.successful_runs.saturating_add(1);
        if usage.failures_today > 0 {
            usage.recovery_success_count = usage.recovery_success_count.saturating_add(1);
            if usage.recovery_success_count >= 3 {
                usage.failures_today = 0;
                usage.recovery_success_count = 0;
            }
        }
    }
    if event.event_type == "task_failed" {
        if usage.failures_today_date != now.date_naive() {
            usage.failures_today_date = now.date_naive();
            usage.failures_today = 0;
        }
        usage.failures_today = usage.failures_today.saturating_add(1);
        usage.recovery_success_count = 0;
    }

    let desk_update = map_event_to_desk(&event.event_type).map(|desk| {
        let update = DeskActivityUpdate {
            tower_id: event.tower_id.clone(),
            floor_id: event.floor_id.clone(),
            department_id: event.department_id.clone(),
            desk_id: desk.to_string(),
            event_type: event.event_type.clone(),
            task_id: event.task_id.clone(),
            agent_id: event.agent_id.clone(),
            active_until: now + Duration::seconds(30),
            updated_at: now,
        };
        let desk_key = format!(
            "{}:{}:{}:{}",
            event.tower_id, event.floor_id, event.department_id, desk
        );
        state.desk_activity.insert(desk_key, update.clone());
        update
    });

    let dept_state = compute_department_state(usage, &event);
    let department_update = DepartmentStateUpdate {
        tower_id: event.tower_id.clone(),
        floor_id: event.floor_id.clone(),
        department_id: event.department_id.clone(),
        state: dept_state.0,
        reason: dept_state.1,
        updated_at: now,
    };
    let state_key = department_state_key(&event.tower_id, &event.floor_id, &event.department_id);
    state
        .department_state
        .insert(state_key, department_update.clone());

    let prestige = floor_prestige_from_usage(usage, false, now);

    let sim_event = SimEvent {
        id: event.event_id,
        tick: 0,
        timestamp: event.timestamp,
        category: "openclaw".to_string(),
        severity: if event.event_type == "task_failed" {
            "high".to_string()
        } else {
            "info".to_string()
        },
        message: format!(
            "{}:{}:{}:{}",
            event.floor_id,
            event.department_id,
            event.event_type,
            event.task_id.clone().unwrap_or_default()
        ),
        refs: vec![
            event.floor_id.clone(),
            event.department_id.clone(),
            event.task_id.clone().unwrap_or_default(),
        ],
    };

    Ok(ProcessOutcome {
        desk_update,
        department_update,
        prestige,
        sim_event,
    })
}

pub fn get_department_state(
    state: &OpsState,
    tower_id: &str,
    floor_id: &str,
    department_id: &str,
) -> Option<DepartmentStateUpdate> {
    state
        .department_state
        .get(&department_state_key(tower_id, floor_id, department_id))
        .cloned()
}

pub fn compute_floor_prestige(state: &OpsState, now: DateTime<Utc>) -> Vec<FloorPrestige> {
    let elite = elite_floor_ids(state);
    state
        .floor_usage
        .values()
        .map(|u| floor_prestige_from_usage(u, elite.contains(&u.floor_id), now))
        .collect()
}

pub fn compute_rankings(state: &OpsState, tower_id: &str, now: DateTime<Utc>) -> Vec<RankingEntry> {
    let mut values = compute_floor_prestige(state, now)
        .into_iter()
        .filter(|p| p.tower_id == tower_id)
        .collect::<Vec<_>>();
    values.sort_by(|a, b| {
        b.prestige
            .partial_cmp(&a.prestige)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.current_streak_days.cmp(&a.current_streak_days))
    });
    values
        .into_iter()
        .enumerate()
        .map(|(i, p)| RankingEntry {
            rank: i + 1,
            tower_id: p.tower_id,
            floor_id: p.floor_id,
            prestige: p.prestige,
            current_streak_days: p.current_streak_days,
        })
        .collect()
}

fn map_event_to_desk(event_type: &str) -> Option<&'static str> {
    match event_type {
        "git_pull" => Some("pull_desk"),
        "git_commit" => Some("commit_desk"),
        "git_push" => Some("push_desk"),
        "test_run" => Some("test_desk"),
        "task_completed" => Some("review_desk"),
        "task_started" | "task_failed" => Some("planning_board"),
        _ => None,
    }
}

fn compute_department_state(
    usage: &FloorUsage,
    event: &OpenClawEvent,
) -> (DepartmentHealth, String) {
    if event.event_type == "task_failed" || usage.failures_today > 0 {
        if usage.recovery_success_count == 0 {
            return (
                DepartmentHealth::Broken,
                "Immediate failure observed".to_string(),
            );
        }
        if usage.recovery_success_count < 3 {
            return (
                DepartmentHealth::Degraded,
                format!(
                    "Recovering: {} successful tasks since failure",
                    usage.recovery_success_count
                ),
            );
        }
    }
    if event.event_type == "task_started" {
        return (
            DepartmentHealth::Healthy,
            "Task execution active".to_string(),
        );
    }
    (DepartmentHealth::Healthy, "Operating normally".to_string())
}

fn refresh_streak(usage: &mut FloorUsage, now: DateTime<Utc>) {
    let today = now.date_naive();
    let diff_days = today
        .signed_duration_since(usage.last_active_date)
        .num_days();
    if diff_days == 1 {
        usage.current_streak_days = usage.current_streak_days.saturating_add(1);
    } else if diff_days > 1 {
        usage.current_streak_days = 1;
    }
    usage.last_active_date = today;
}

fn floor_prestige_from_usage(usage: &FloorUsage, elite: bool, now: DateTime<Utc>) -> FloorPrestige {
    let mut score = usage.successful_runs as f32
        + (usage.active_runtime_seconds as f32 / 3600.0) * 2.0
        + usage.current_streak_days as f32 * 5.0;
    let idle_hours = now
        .signed_duration_since(usage.last_active_at)
        .num_hours()
        .max(0);
    let decay_window_hours = if elite { 24 * 7 } else { 72 };
    if idle_hours > decay_window_hours {
        let extra = (idle_hours - decay_window_hours) as f32;
        score = (score - extra * 0.1).max(0.0);
    }
    FloorPrestige {
        tower_id: usage.tower_id.clone(),
        floor_id: usage.floor_id.clone(),
        prestige: score,
        successful_runs: usage.successful_runs,
        active_runtime_seconds: usage.active_runtime_seconds,
        current_streak_days: usage.current_streak_days,
        last_active_at: usage.last_active_at,
        decay_window_hours,
    }
}

fn elite_floor_ids(state: &OpsState) -> Vec<String> {
    let mut floors = state.floor_usage.values().collect::<Vec<_>>();
    floors.sort_by(|a, b| b.current_streak_days.cmp(&a.current_streak_days));
    let count = floors.len();
    if count == 0 {
        return vec![];
    }
    let elite_count = ((count as f32) * 0.1).ceil().max(1.0) as usize;
    floors
        .into_iter()
        .take(elite_count)
        .map(|f| f.floor_id.clone())
        .collect()
}

fn floor_usage_key(tower_id: &str, floor_id: &str) -> String {
    format!("{tower_id}:{floor_id}")
}

fn department_state_key(tower_id: &str, floor_id: &str, department_id: &str) -> String {
    format!("{tower_id}:{floor_id}:{department_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_failed_immediately_breaks_department() {
        let mut state = OpsState::default();
        let event = OpenClawEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            tower_id: "tower-1".to_string(),
            floor_id: "floor-a".to_string(),
            department_id: CODING_DEPARTMENT.to_string(),
            event_type: "task_failed".to_string(),
            task_id: Some("t1".to_string()),
            agent_id: Some("a1".to_string()),
            metadata: None,
        };
        let out = process_event(&mut state, event, Utc::now()).expect("processed");
        assert!(matches!(
            out.department_update.state,
            DepartmentHealth::Broken
        ));
    }

    #[test]
    fn rankings_are_usage_based() {
        let mut state = OpsState::default();
        let now = Utc::now();
        for (floor, times) in [("f1", 3), ("f2", 1)] {
            for _ in 0..times {
                let event = OpenClawEvent {
                    event_id: Uuid::new_v4(),
                    timestamp: now,
                    tower_id: "tower-1".to_string(),
                    floor_id: floor.to_string(),
                    department_id: CODING_DEPARTMENT.to_string(),
                    event_type: "task_completed".to_string(),
                    task_id: Some("x".to_string()),
                    agent_id: Some("a".to_string()),
                    metadata: None,
                };
                let _ = process_event(&mut state, event, now);
            }
        }
        let ranks = compute_rankings(&state, "tower-1", now);
        assert_eq!(ranks.first().map(|r| r.floor_id.as_str()), Some("f1"));
    }
}
