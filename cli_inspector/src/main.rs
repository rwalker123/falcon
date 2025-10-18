use clap::Parser;
use color_eyre::Result;

/// Basic CLI inspector stub that will connect to the headless simulation and
/// render state in the terminal. For now it prints a placeholder message to
/// confirm the repo scaffold is wired correctly.
#[derive(Parser, Debug)]
#[command(author, version, about = "Shadow-Scale CLI inspector prototype", long_about = None)]
struct Cli {
    /// Optional address of the headless simulation server.
    #[arg(long, default_value = "127.0.0.1:41000")]
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    println!("Connecting to simulation at {} (stub)...", cli.endpoint);
    println!("CLI inspector is not yet implemented.");
    Ok(())
}
