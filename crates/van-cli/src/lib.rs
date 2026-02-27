mod cmd;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "van", version, about = "Van - Vue-like template engine toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Van project
    Init {
        /// Project name (optional, will prompt if not provided)
        name: Option<String>,
    },
    /// Start development server
    Dev,
    /// Generate static HTML pages
    Generate,
}

pub async fn run() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { name } => cmd::init::run(name),
        Commands::Dev => cmd::dev::run().await,
        Commands::Generate => cmd::generate::run(),
    };

    if let Err(e) = result {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
