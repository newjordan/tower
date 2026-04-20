use crate::{
    ops::{FloorUsage, OpsState},
    sim::{
        DepartmentHealth, DepartmentStateUpdate, DeskActivityUpdate, MetricsPoint, SimEvent,
        TowerState,
    },
};
use chrono::{NaiveDate, Utc};
use redis::AsyncCommands;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct Storage {
    pg: Option<PgPool>,
    redis: Option<redis::Client>,
}

impl Storage {
    pub async fn initialize() -> Self {
        let pg = init_postgres().await;
        let redis = init_redis().await;
        Storage { pg, redis }
    }

    pub fn postgres_enabled(&self) -> bool {
        self.pg.is_some()
    }

    pub fn redis_enabled(&self) -> bool {
        self.redis.is_some()
    }

    pub async fn load_latest_snapshot(&self, tower_id: &str) -> Option<TowerState> {
        if let Some(redis_client) = &self.redis {
            if let Ok(mut conn) = redis_client.get_multiplexed_async_connection().await {
                let key = format!("tower:{tower_id}:snapshot");
                match conn.get::<_, Option<String>>(&key).await {
                    Ok(Some(v)) => match serde_json::from_str::<TowerState>(&v) {
                        Ok(state) => return Some(state),
                        Err(e) => warn!("failed to deserialize redis snapshot: {e}"),
                    },
                    Ok(None) => {}
                    Err(e) => warn!("failed to read redis snapshot: {e}"),
                }
            }
        }

        if let Some(pg_pool) = &self.pg {
            let row = sqlx::query_scalar::<_, serde_json::Value>(
                r#"
                SELECT snapshot
                FROM tower_snapshots
                WHERE tower_id = $1
                ORDER BY tick DESC
                LIMIT 1
                "#,
            )
            .bind(tower_id)
            .fetch_optional(pg_pool)
            .await;

            match row {
                Ok(Some(value)) => match serde_json::from_value::<TowerState>(value) {
                    Ok(state) => return Some(state),
                    Err(e) => warn!("failed to deserialize postgres snapshot: {e}"),
                },
                Ok(None) => {}
                Err(e) => warn!("failed to read postgres snapshot: {e}"),
            }
        }

        None
    }

    pub async fn persist_tick(
        &self,
        world: &TowerState,
        metrics: &MetricsPoint,
        events: &[SimEvent],
        persist_snapshot: bool,
    ) {
        if let Some(redis_client) = &self.redis {
            match redis_client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    let key = format!("tower:{}:snapshot", world.tower_id);
                    if let Ok(snapshot_json) = serde_json::to_string(world) {
                        if let Err(e) = conn.set::<_, _, ()>(key, snapshot_json).await {
                            warn!("redis set snapshot failed: {e}");
                        }
                    }
                }
                Err(e) => warn!("redis connection failed: {e}"),
            }
        }

        if let Some(pg_pool) = &self.pg {
            if let Err(e) = persist_metrics(pg_pool, &world.tower_id, metrics).await {
                warn!("persist metrics failed: {e}");
            }
            if !events.is_empty() {
                if let Err(e) = persist_events(pg_pool, &world.tower_id, events).await {
                    warn!("persist events failed: {e}");
                }
            }
            if persist_snapshot {
                if let Err(e) = persist_snapshot_row(pg_pool, world).await {
                    warn!("persist snapshot failed: {e}");
                }
            }
        }
    }

    pub async fn load_recent_metrics(&self, tower_id: &str, limit: i64) -> Vec<MetricsPoint> {
        let Some(pool) = &self.pg else {
            return vec![];
        };
        let rows = sqlx::query(
            r#"
            SELECT tick, metric_time, occupancy, avg_need_satisfaction, liquidity, alert_count, tower_health
            FROM tower_metrics
            WHERE tower_id = $1
            ORDER BY tick DESC
            LIMIT $2
            "#,
        )
        .bind(tower_id)
        .bind(limit)
        .fetch_all(pool)
        .await;
        match rows {
            Ok(rows) => {
                let mut out = Vec::with_capacity(rows.len());
                for row in rows.into_iter().rev() {
                    out.push(MetricsPoint {
                        tick: row.try_get::<i64, _>("tick").unwrap_or_default() as u64,
                        timestamp: row
                            .try_get::<chrono::DateTime<Utc>, _>("metric_time")
                            .unwrap_or_else(|_| Utc::now()),
                        occupancy: row.try_get::<f32, _>("occupancy").unwrap_or_default(),
                        avg_need_satisfaction: row
                            .try_get::<f32, _>("avg_need_satisfaction")
                            .unwrap_or_default(),
                        liquidity: row.try_get::<f32, _>("liquidity").unwrap_or_default(),
                        alert_count: row.try_get::<i32, _>("alert_count").unwrap_or_default()
                            as usize,
                        tower_health: row.try_get::<f32, _>("tower_health").unwrap_or_default(),
                    });
                }
                out
            }
            Err(e) => {
                warn!("failed to load metrics from postgres: {e}");
                vec![]
            }
        }
    }

    pub async fn load_recent_events(&self, tower_id: &str, limit: i64) -> Vec<SimEvent> {
        let Some(pool) = &self.pg else {
            return vec![];
        };
        let rows = sqlx::query(
            r#"
            SELECT id, tick, event_time, category, severity, message, refs
            FROM tower_events
            WHERE tower_id = $1
            ORDER BY tick DESC
            LIMIT $2
            "#,
        )
        .bind(tower_id)
        .bind(limit)
        .fetch_all(pool)
        .await;
        match rows {
            Ok(rows) => {
                let mut out = Vec::with_capacity(rows.len());
                for row in rows.into_iter().rev() {
                    let refs = row
                        .try_get::<serde_json::Value, _>("refs")
                        .ok()
                        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                        .unwrap_or_default();
                    out.push(SimEvent {
                        id: row.try_get("id").unwrap_or_else(|_| uuid::Uuid::new_v4()),
                        tick: row.try_get::<i64, _>("tick").unwrap_or_default() as u64,
                        timestamp: row
                            .try_get::<chrono::DateTime<Utc>, _>("event_time")
                            .unwrap_or_else(|_| Utc::now()),
                        category: row
                            .try_get("category")
                            .unwrap_or_else(|_| "unknown".to_string()),
                        severity: row
                            .try_get("severity")
                            .unwrap_or_else(|_| "info".to_string()),
                        message: row.try_get("message").unwrap_or_else(|_| "".to_string()),
                        refs,
                    });
                }
                out
            }
            Err(e) => {
                warn!("failed to load events from postgres: {e}");
                vec![]
            }
        }
    }

    pub async fn persist_ops_state(&self, state: &OpsState) {
        let Some(pool) = &self.pg else {
            return;
        };

        for update in state.department_state.values() {
            let _ = sqlx::query(
                r#"
                INSERT INTO ops_department_states (
                    tower_id, floor_id, department_id, state, reason, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (tower_id, floor_id, department_id)
                DO UPDATE SET
                    state = EXCLUDED.state,
                    reason = EXCLUDED.reason,
                    updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(&update.tower_id)
            .bind(&update.floor_id)
            .bind(&update.department_id)
            .bind(department_health_to_str(&update.state))
            .bind(&update.reason)
            .bind(update.updated_at)
            .execute(pool)
            .await;
        }

        for desk in state.desk_activity.values() {
            let _ = sqlx::query(
                r#"
                INSERT INTO ops_desk_activity (
                    tower_id, floor_id, department_id, desk_id, event_type, task_id, agent_id, active_until, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (tower_id, floor_id, department_id, desk_id)
                DO UPDATE SET
                    event_type = EXCLUDED.event_type,
                    task_id = EXCLUDED.task_id,
                    agent_id = EXCLUDED.agent_id,
                    active_until = EXCLUDED.active_until,
                    updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(&desk.tower_id)
            .bind(&desk.floor_id)
            .bind(&desk.department_id)
            .bind(&desk.desk_id)
            .bind(&desk.event_type)
            .bind(&desk.task_id)
            .bind(&desk.agent_id)
            .bind(desk.active_until)
            .bind(desk.updated_at)
            .execute(pool)
            .await;
        }

        for usage in state.floor_usage.values() {
            let _ = sqlx::query(
                r#"
                INSERT INTO ops_floor_usage (
                    tower_id, floor_id, successful_runs, active_runtime_seconds, current_streak_days,
                    last_active_date, last_active_at, failures_today_date, failures_today, recovery_success_count, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT (tower_id, floor_id)
                DO UPDATE SET
                    successful_runs = EXCLUDED.successful_runs,
                    active_runtime_seconds = EXCLUDED.active_runtime_seconds,
                    current_streak_days = EXCLUDED.current_streak_days,
                    last_active_date = EXCLUDED.last_active_date,
                    last_active_at = EXCLUDED.last_active_at,
                    failures_today_date = EXCLUDED.failures_today_date,
                    failures_today = EXCLUDED.failures_today,
                    recovery_success_count = EXCLUDED.recovery_success_count,
                    updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(&usage.tower_id)
            .bind(&usage.floor_id)
            .bind(usage.successful_runs as i64)
            .bind(usage.active_runtime_seconds as i64)
            .bind(usage.current_streak_days as i32)
            .bind(usage.last_active_date)
            .bind(usage.last_active_at)
            .bind(usage.failures_today_date)
            .bind(usage.failures_today as i32)
            .bind(usage.recovery_success_count as i32)
            .bind(Utc::now())
            .execute(pool)
            .await;
        }
    }

    pub async fn load_ops_state(&self, tower_id: &str) -> OpsState {
        let Some(pool) = &self.pg else {
            return OpsState::default();
        };
        let mut state = OpsState::default();

        let dept_rows = sqlx::query(
            r#"
            SELECT tower_id, floor_id, department_id, state, reason, updated_at
            FROM ops_department_states
            WHERE tower_id = $1
            "#,
        )
        .bind(tower_id)
        .fetch_all(pool)
        .await;
        if let Ok(rows) = dept_rows {
            for row in rows {
                let tower: String = row.try_get("tower_id").unwrap_or_default();
                let floor: String = row.try_get("floor_id").unwrap_or_default();
                let dept: String = row.try_get("department_id").unwrap_or_default();
                let state_str: String = row
                    .try_get("state")
                    .unwrap_or_else(|_| "healthy".to_string());
                let update = DepartmentStateUpdate {
                    tower_id: tower.clone(),
                    floor_id: floor.clone(),
                    department_id: dept.clone(),
                    state: parse_department_health(&state_str),
                    reason: row
                        .try_get("reason")
                        .unwrap_or_else(|_| "Hydrated".to_string()),
                    updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
                };
                state
                    .department_state
                    .insert(format!("{tower}:{floor}:{dept}"), update);
            }
        }

        let desk_rows = sqlx::query(
            r#"
            SELECT tower_id, floor_id, department_id, desk_id, event_type, task_id, agent_id, active_until, updated_at
            FROM ops_desk_activity
            WHERE tower_id = $1
            "#,
        )
        .bind(tower_id)
        .fetch_all(pool)
        .await;
        if let Ok(rows) = desk_rows {
            for row in rows {
                let tower: String = row.try_get("tower_id").unwrap_or_default();
                let floor: String = row.try_get("floor_id").unwrap_or_default();
                let dept: String = row.try_get("department_id").unwrap_or_default();
                let desk: String = row.try_get("desk_id").unwrap_or_default();
                let update = DeskActivityUpdate {
                    tower_id: tower.clone(),
                    floor_id: floor.clone(),
                    department_id: dept.clone(),
                    desk_id: desk.clone(),
                    event_type: row.try_get("event_type").unwrap_or_default(),
                    task_id: row.try_get("task_id").ok(),
                    agent_id: row.try_get("agent_id").ok(),
                    active_until: row.try_get("active_until").unwrap_or_else(|_| Utc::now()),
                    updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
                };
                state
                    .desk_activity
                    .insert(format!("{tower}:{floor}:{dept}:{desk}"), update);
            }
        }

        let usage_rows = sqlx::query(
            r#"
            SELECT tower_id, floor_id, successful_runs, active_runtime_seconds, current_streak_days,
                   last_active_date, last_active_at, failures_today_date, failures_today, recovery_success_count
            FROM ops_floor_usage
            WHERE tower_id = $1
            "#,
        )
        .bind(tower_id)
        .fetch_all(pool)
        .await;
        if let Ok(rows) = usage_rows {
            for row in rows {
                let tower: String = row.try_get("tower_id").unwrap_or_default();
                let floor: String = row.try_get("floor_id").unwrap_or_default();
                let usage = FloorUsage {
                    tower_id: tower.clone(),
                    floor_id: floor.clone(),
                    successful_runs: row.try_get::<i64, _>("successful_runs").unwrap_or_default()
                        as u64,
                    active_runtime_seconds: row
                        .try_get::<i64, _>("active_runtime_seconds")
                        .unwrap_or_default() as u64,
                    current_streak_days: row.try_get::<i32, _>("current_streak_days").unwrap_or(1)
                        as u32,
                    last_active_date: row
                        .try_get::<NaiveDate, _>("last_active_date")
                        .unwrap_or_else(|_| Utc::now().date_naive()),
                    last_active_at: row.try_get("last_active_at").unwrap_or_else(|_| Utc::now()),
                    failures_today_date: row
                        .try_get::<NaiveDate, _>("failures_today_date")
                        .unwrap_or_else(|_| Utc::now().date_naive()),
                    failures_today: row.try_get::<i32, _>("failures_today").unwrap_or_default()
                        as u32,
                    recovery_success_count: row
                        .try_get::<i32, _>("recovery_success_count")
                        .unwrap_or_default() as u32,
                };
                state.floor_usage.insert(format!("{tower}:{floor}"), usage);
            }
        }

        state
    }
}

async fn init_postgres() -> Option<PgPool> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => return None,
    };
    match PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            if let Err(e) = ensure_schema(&pool).await {
                error!("postgres schema setup failed: {e}");
                return None;
            }
            info!("postgres storage enabled");
            Some(pool)
        }
        Err(e) => {
            error!("postgres connect failed: {e}");
            None
        }
    }
}

async fn init_redis() -> Option<redis::Client> {
    let redis_url = match std::env::var("REDIS_URL") {
        Ok(v) => v,
        Err(_) => return None,
    };
    match redis::Client::open(redis_url) {
        Ok(client) => match client.get_multiplexed_async_connection().await {
            Ok(mut conn) => {
                let pong: redis::RedisResult<String> =
                    redis::cmd("PING").query_async(&mut conn).await;
                match pong {
                    Ok(_) => info!("redis cache enabled"),
                    Err(e) => warn!("redis ping failed: {e}"),
                }
                Some(client)
            }
            Err(e) => {
                error!("redis connect failed: {e}");
                None
            }
        },
        Err(e) => {
            error!("redis client init failed: {e}");
            None
        }
    }
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tower_snapshots (
            id BIGSERIAL PRIMARY KEY,
            tower_id TEXT NOT NULL,
            tick BIGINT NOT NULL,
            captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            snapshot JSONB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tower_snapshots_tower_tick
            ON tower_snapshots (tower_id, tick DESC);

        CREATE TABLE IF NOT EXISTS tower_events (
            id UUID PRIMARY KEY,
            tower_id TEXT NOT NULL,
            tick BIGINT NOT NULL,
            event_time TIMESTAMPTZ NOT NULL,
            category TEXT NOT NULL,
            severity TEXT NOT NULL,
            message TEXT NOT NULL,
            refs JSONB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tower_events_tower_tick
            ON tower_events (tower_id, tick DESC);

        CREATE TABLE IF NOT EXISTS tower_metrics (
            id BIGSERIAL PRIMARY KEY,
            tower_id TEXT NOT NULL,
            tick BIGINT NOT NULL,
            metric_time TIMESTAMPTZ NOT NULL,
            occupancy REAL NOT NULL,
            avg_need_satisfaction REAL NOT NULL,
            liquidity REAL NOT NULL,
            alert_count INTEGER NOT NULL,
            tower_health REAL NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tower_metrics_tower_tick
            ON tower_metrics (tower_id, tick DESC);

        CREATE TABLE IF NOT EXISTS ops_department_states (
            tower_id TEXT NOT NULL,
            floor_id TEXT NOT NULL,
            department_id TEXT NOT NULL,
            state TEXT NOT NULL,
            reason TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (tower_id, floor_id, department_id)
        );

        CREATE TABLE IF NOT EXISTS ops_desk_activity (
            tower_id TEXT NOT NULL,
            floor_id TEXT NOT NULL,
            department_id TEXT NOT NULL,
            desk_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            task_id TEXT NULL,
            agent_id TEXT NULL,
            active_until TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (tower_id, floor_id, department_id, desk_id)
        );

        CREATE TABLE IF NOT EXISTS ops_floor_usage (
            tower_id TEXT NOT NULL,
            floor_id TEXT NOT NULL,
            successful_runs BIGINT NOT NULL,
            active_runtime_seconds BIGINT NOT NULL,
            current_streak_days INTEGER NOT NULL,
            last_active_date DATE NOT NULL,
            last_active_at TIMESTAMPTZ NOT NULL,
            failures_today_date DATE NOT NULL,
            failures_today INTEGER NOT NULL,
            recovery_success_count INTEGER NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (tower_id, floor_id)
        );
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn persist_snapshot_row(pool: &PgPool, world: &TowerState) -> Result<(), sqlx::Error> {
    let snapshot = serde_json::to_value(world).unwrap_or_else(|_| serde_json::json!({}));
    sqlx::query(
        r#"
        INSERT INTO tower_snapshots (tower_id, tick, captured_at, snapshot)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&world.tower_id)
    .bind(world.tick as i64)
    .bind(Utc::now())
    .bind(snapshot)
    .execute(pool)
    .await?;
    Ok(())
}

async fn persist_events(
    pool: &PgPool,
    tower_id: &str,
    events: &[SimEvent],
) -> Result<(), sqlx::Error> {
    for event in events {
        let refs = serde_json::to_value(&event.refs).unwrap_or_else(|_| serde_json::json!([]));
        sqlx::query(
            r#"
            INSERT INTO tower_events (id, tower_id, tick, event_time, category, severity, message, refs)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(event.id)
        .bind(tower_id)
        .bind(event.tick as i64)
        .bind(event.timestamp)
        .bind(&event.category)
        .bind(&event.severity)
        .bind(&event.message)
        .bind(refs)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn persist_metrics(
    pool: &PgPool,
    tower_id: &str,
    metric: &MetricsPoint,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO tower_metrics (
            tower_id, tick, metric_time, occupancy, avg_need_satisfaction, liquidity, alert_count, tower_health
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(tower_id)
    .bind(metric.tick as i64)
    .bind(metric.timestamp)
    .bind(metric.occupancy)
    .bind(metric.avg_need_satisfaction)
    .bind(metric.liquidity)
    .bind(metric.alert_count as i32)
    .bind(metric.tower_health)
    .execute(pool)
    .await?;
    Ok(())
}

fn department_health_to_str(state: &DepartmentHealth) -> &'static str {
    match state {
        DepartmentHealth::Healthy => "healthy",
        DepartmentHealth::Degraded => "degraded",
        DepartmentHealth::Stalled => "stalled",
        DepartmentHealth::Broken => "broken",
    }
}

fn parse_department_health(s: &str) -> DepartmentHealth {
    match s {
        "degraded" => DepartmentHealth::Degraded,
        "stalled" => DepartmentHealth::Stalled,
        "broken" => DepartmentHealth::Broken,
        _ => DepartmentHealth::Healthy,
    }
}
