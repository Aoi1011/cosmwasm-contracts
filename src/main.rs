use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Decode { value: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Decode { value } => {
            let decoded: String = serde_bencode::from_str(&value).context("decode value")?;
            println!("{decoded}");
        }
    }

    Ok(())
}
