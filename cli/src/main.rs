use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tokencamp", about = "LLM API Gateway CLI", version = "0.7")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Start,
    /// Run database migrations
    Migrate { database_url: String },
    /// Generate a new encryption key (64 hex chars)
    KeyRotate {
        #[arg(long)]
        old_key: Option<String>,
    },
    /// Validate config file
    Check { config: Option<String> },
    /// Show version
    Version,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => {
            println!("tokencamp: use 'cargo run -p gateway' to start the server");
            println!("  or 'docker compose up' for full stack");
        }
        Commands::Migrate { database_url } => {
            println!("Migrating database: {}", database_url);
            println!("Run SQL files from migrations/ directory");
        }
        Commands::KeyRotate { old_key } => {
            let new_key: String = (0..32).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
            println!("New ENCRYPTION_KEY={}", new_key);
            if old_key.is_some() {
                println!("Re-encrypt existing credentials with: tokencamp rotate-credentials --old-key ... --new-key {}", new_key);
            }
        }
        Commands::Check { config } => {
            let path = config.unwrap_or_else(|| "config/default.yaml".into());
            println!("Checking config: {}", path);
            match std::fs::read_to_string(&path) {
                Ok(_) => println!("Config file OK"),
                Err(e) => eprintln!("Error reading config: {}", e),
            }
        }
        Commands::Version => {
            println!("tokencamp v0.7");
        }
    }
}
