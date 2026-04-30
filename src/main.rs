use anyhow::Result;
use clap::Parser;

use lazytask::app::App;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut app = App::new(cli.config.as_deref(), cli.verbose).await?;
    app.run().await?;

    Ok(())
}
