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
    /// Build for production
    Build,
    /// Generate static HTML pages
    Generate,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { name } => cmd::init::run(name),
        Commands::Dev => cmd::dev::run().await,
        Commands::Build => cmd::build::run(),
        Commands::Generate => cmd::generate::run(),
    };

    if let Err(e) = result {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
