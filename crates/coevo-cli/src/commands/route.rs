use coevo_core::contract::MCLSpec;
use coevo_router::pcdt::PcdtRouter;

pub async fn run(contract_file: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let contract: MCLSpec = if let Some(path) = contract_file {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        // Read from stdin
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        serde_json::from_str(&input)?
    };

    let agents = vec![
        "agent-synthesizer-01".to_string(),
        "agent-critic-01".to_string(),
    ];
    let result = PcdtRouter::compute(&contract, agents, None)
        .map_err(|e| format!("routing failed: {}", e))?;

    println!("Plan Hash: {}", result.plan_hash);
    println!(
        "Plan JSON:\n{}",
        serde_json::to_string_pretty(&result.plan)?
    );
    Ok(())
}
