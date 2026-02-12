use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "kicodex",
    about = "KiCad HTTP Library server backed by CSV files"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP server for a library directory
    Serve {
        /// Path to the library directory (containing library.yaml)
        path: PathBuf,

        /// Port to listen on
        #[arg(long, default_value_t = 18734)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { path, port } => {
            let path = path.canonicalize().unwrap_or(path);
            kicodex_core::server::run_server(&path, port).await?;
        }
    }

    Ok(())
}
