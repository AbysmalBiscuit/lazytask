use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use lazytask::app::App;
use lazytask::config::Config;
use lazytask::schema::{self, InitOutcome};

#[derive(Parser)]
#[command(
    name = "lazytask",
    about = "A modern Terminal User Interface for Taskwarrior",
    version
)]
struct Cli {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the JSON schema for the config file
    Schema {
        #[command(subcommand)]
        action: Option<SchemaAction>,
    },
}

#[derive(Subcommand)]
enum SchemaAction {
    /// Add the `#:schema` header to a config, or write a commented-out
    /// starter when none exists
    Init {
        /// Config to update; defaults to `--config`, then the default config path
        path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            let mut app = App::new(cli.config.as_deref(), cli.verbose).await?;
            app.run().await?;
        }
        Some(Command::Schema { action: None }) => print!("{}", schema::document()?),
        Some(Command::Schema {
            action: Some(SchemaAction::Init { path }),
        }) => {
            let path = match path.or(cli.config.map(PathBuf::from)) {
                Some(path) => path,
                None => Config::default_config_path()?,
            };
            let message = match schema::init(&path)? {
                InitOutcome::AlreadyLinked => "already has a #:schema header, left unchanged",
                InitOutcome::AddedHeader => "now points at the lazytask schema",
                InitOutcome::WroteStarter => "written with every setting commented out",
            };
            println!("{}: {message}", path.display());
        }
    }

    Ok(())
}
