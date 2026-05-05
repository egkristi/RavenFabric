use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rf", about = "RavenFabric — secure remote execution")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a command on a remote agent
    Exec {
        /// Target agent ID
        agent: String,
        /// Command to execute
        command: String,
    },
    /// Start local development mode (agent + relay, no auth)
    Dev,
    /// Show connected agents and status
    Status,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Exec { agent, command } => {
            println!("rf exec {}: \"{}\" — not yet implemented", agent, command);
        }
        Commands::Dev => {
            println!("rf dev — not yet implemented");
        }
        Commands::Status => {
            println!("rf status — not yet implemented");
        }
    }
}
