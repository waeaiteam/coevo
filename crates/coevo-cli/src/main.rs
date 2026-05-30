//! coevo CLI — local command-line interface.
//! Per whitepaper requirement.

mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coevo", about = "coevo Agent Governance Mesh CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compile user intent into MCL contract
    Compile {
        /// Natural language intent
        intent: String,
        /// Execution mode: DRAFT or ACTIVE
        #[arg(short, long, default_value = "DRAFT")]
        mode: String,
    },
    /// Route a contract (requires contract JSON on stdin or file)
    Route {
        /// Path to contract JSON file
        #[arg(short, long)]
        contract_file: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { intent, mode } => {
            commands::compile::run(&intent, &mode).await?;
        }
        Commands::Route { contract_file } => {
            commands::route::run(contract_file).await?;
        }
    }

    Ok(())
}
