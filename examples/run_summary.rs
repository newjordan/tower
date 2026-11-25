//! Print a readable summary of a seeded tower run.
//!
//! ```text
//! cargo run --example run_summary -- --seed 42 --ticks 200
//! ```
//!
//! The same seed and tick count print the same state fingerprint.

use agent_tower_sim::sim::{
    AgentGoal, FloorStage, FloorType, compute_metrics, initialize_tower, run_tick,
    state_fingerprint,
};
use std::time::Instant;

fn main() {
    let (seed, ticks) = parse_args();
    let mut state = initialize_tower(seed);
    let started = Instant::now();
    for _ in 0..ticks {
        run_tick(&mut state);
    }
    let elapsed = started.elapsed();
    let fingerprint = state_fingerprint(&state);
    let metrics = compute_metrics(&state);
    let ticks_per_sec = if elapsed.as_secs_f64() > 0.0 {
        f64::from(ticks) / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!("Agent tower run");
    println!("  seed              {seed}");
    println!("  ticks             {ticks}");
    println!("  elapsed           {:.2?}", elapsed);
    println!("  tick throughput   {ticks_per_sec:.0} ticks/s");
    println!("  fingerprint       {fingerprint}");
    println!();
    println!("Tower");
    println!("  id                {}", state.tower_id);
    println!("  tick              {}", state.tick);
    println!("  agents            {}", state.agents.len());
    println!("  floors            {}", state.floors.len());
    println!("  expansions        {}", state.expansion_count);
    println!("  alerts            {}", state.alerts.len());
    println!();
    println!("Last tick economy");
    println!("  wages paid        {}", state.economy.wages_paid);
    println!("  rent collected    {}", state.economy.rent_collected);
    println!("  service spend     {}", state.economy.service_spend);
    println!("  unmet penalty     {}", state.economy.unmet_need_penalty);
    println!();
    println!("Metrics");
    println!("  occupancy         {:.3}", metrics.occupancy);
    println!("  avg satisfaction  {:.2}", metrics.avg_need_satisfaction);
    println!("  liquidity         {:.2}", metrics.liquidity);
    println!("  tower health      {:.2}", metrics.tower_health);
    println!();
    print_goals(&state.agents);
    println!();
    print_floors(&state.floors);
    if !state.alerts.is_empty() {
        println!();
        println!("Alerts");
        for alert in state.alerts.iter().take(8) {
            println!("  {:<8} {}", alert.severity, alert.message);
        }
    }
}

fn print_goals(agents: &[agent_tower_sim::sim::Agent]) {
    let mut work = 0;
    let mut eat = 0;
    let mut rest = 0;
    let mut socialize = 0;
    let mut relocate = 0;
    for agent in agents {
        match agent.current_goal {
            AgentGoal::Work => work += 1,
            AgentGoal::Eat => eat += 1,
            AgentGoal::Rest => rest += 1,
            AgentGoal::Socialize => socialize += 1,
            AgentGoal::Relocate => relocate += 1,
        }
    }
    println!("Goals");
    println!("  work              {work}");
    println!("  eat               {eat}");
    println!("  rest              {rest}");
    println!("  socialize         {socialize}");
    println!("  relocate          {relocate}");
}

fn print_floors(floors: &[agent_tower_sim::sim::Floor]) {
    println!("Floors");
    println!(
        "  {:<22} {:<12} {:<11} {:>7} {:>5} {:>5}  pipeline",
        "name", "type", "stage", "occ/cap", "score", "furn"
    );
    for floor in floors {
        let pipeline = floor
            .manager
            .pipeline
            .as_ref()
            .map(|pipeline| match pipeline.kind {
                agent_tower_sim::sim::PipelineKind::CiCd => "ci_cd",
                agent_tower_sim::sim::PipelineKind::MailRoom => "mail_room",
                agent_tower_sim::sim::PipelineKind::FoodService => "food_service",
            })
            .unwrap_or("-");
        println!(
            "  {:<22} {:<12} {:<11} {:>3}/{:<3} {:>5} {:>5}  {pipeline}",
            floor.name,
            floor_type_label(floor.floor_type),
            stage_label(floor.stage),
            floor.occupancy,
            floor.capacity,
            floor.stage_score,
            floor.manager.layout.furniture.len()
        );
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

fn stage_label(stage: FloorStage) -> &'static str {
    match stage {
        FloorStage::Seed => "seed",
        FloorStage::Growing => "growing",
        FloorStage::Mature => "mature",
        FloorStage::Overloaded => "overloaded",
    }
}

fn parse_args() -> (u64, u32) {
    let mut seed = 42u64;
    let mut ticks = 200u32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                eprintln!("Usage: run_summary [--seed N] [--ticks N]");
                std::process::exit(0);
            }
            "--seed" => {
                let Some(value) = args.next() else {
                    fail("missing value for --seed");
                };
                seed = value.parse().unwrap_or_else(|_| fail("invalid --seed"));
            }
            "--ticks" => {
                let Some(value) = args.next() else {
                    fail("missing value for --ticks");
                };
                ticks = value.parse().unwrap_or_else(|_| fail("invalid --ticks"));
            }
            other if other.starts_with("--seed=") => {
                seed = other["--seed=".len()..]
                    .parse()
                    .unwrap_or_else(|_| fail("invalid --seed"));
            }
            other if other.starts_with("--ticks=") => {
                ticks = other["--ticks=".len()..]
                    .parse()
                    .unwrap_or_else(|_| fail("invalid --ticks"));
            }
            other => fail(&format!("unknown argument {other}")),
        }
    }
    (seed, ticks)
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!("Usage: run_summary [--seed N] [--ticks N]");
    std::process::exit(2);
}
