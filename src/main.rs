use std::path::PathBuf;

use anyhow::Context;
use bittorrent_cli::torrent::{Keys, Torrent};
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
    Info { torrent: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Decode { value } => {
            let decoded: String = serde_bencode::from_str(&value).context("decode value")?;
            println!("{decoded}");
        }
        Commands::Info { torrent } => {
            let t_bytes = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent = serde_bencode::from_bytes(&t_bytes).context("parse torrent file")?;

            let file_length = match t.info.keys {
                Keys::SingleFile { length } => length,
                Keys::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
            };

            println!("Tracker URL: {}", t.announce);
            println!("Length: {}", file_length);

            let info_hash = t.info_hash();
            println!("Info Hash: {}", hex::encode(info_hash));

            println!("Piece Hashes:");
            for piece in t.info.pieces.0 {
                println!("{}", hex::encode(piece));
            }
        }
    }

    Ok(())
}
