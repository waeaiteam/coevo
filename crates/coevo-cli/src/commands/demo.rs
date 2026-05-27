use std::process::Command;

pub async fn run(track: &str) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = match track {
        "green" => "/demo/green",
        "yellow" => "/demo/yellow",
        "red" => "/demo/red",
        _ => {
            eprintln!("Unknown track: {}. Use green, yellow, or red.", track);
            return Ok(());
        }
    };

    let url = format!("http://127.0.0.1:8717{}", endpoint);
    println!("Calling {} ...", url);

    let output = Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            &url,
            "-H", "Content-Type: application/json",
            "-H", "x-coevo-tenant-id: cli-demo",
            "-H", "x-coevo-actor-role: CLI",
            "-H", "x-coevo-contract-hash: 0000000000000000000000000000000000000000000000000000000000000000",
            "-H", "x-coevo-policy-version: 0000000000000000000000000000000000000000000000000000000000000000",
            "-H", "x-coevo-execution-plan-hash: 0000000000000000000000000000000000000000000000000000000000000000",
            "-H", format!("x-coevo-causality-parent-id: {}", uuid::Uuid::new_v4()).as_str(),
            "-H", format!("x-coevo-idempotency-key: {}", uuid::Uuid::new_v4()).as_str(),
            "-d", r#"{"tenant_id":"cli-demo","agent_ids":["agent-synthesizer-01"]}"#,
        ])
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("{}", stdout);

        // Pretty print if JSON
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&stdout) {
            println!("\n--- Pretty ---");
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
    } else {
        eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}
