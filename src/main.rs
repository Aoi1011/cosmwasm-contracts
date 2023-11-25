use std::path::PathBuf;

use anyhow::Context;
use bittorrent_cli::{
    torrent::{Keys, Torrent},
    tracker,
};
use clap::{Parser, Subcommand};
use tokio::net::UdpSocket;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Decode {
        value: String,
    },
    Info {
        torrent: PathBuf,
    },
    Peers {
        #[arg(long, short)]
        torrent: PathBuf,

        #[arg(long, short)]
        udp: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        Commands::Peers { torrent, udp } => {
            let t_bytes = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent = serde_bencode::from_bytes(&t_bytes).context("parse torrent file")?;

            let file_length = match t.info.keys {
                Keys::SingleFile { length } => length,
                Keys::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
            };
            println!("Tracker URL: {}", t.announce);
            let info_hash = t.info_hash();
            let request = tracker::Request::new(&info_hash, file_length);

            if udp {
                let socket = UdpSocket::bind("0.0.0.0:8080").await?;
                let request = bincode::serialize(&request)?;
                socket.send_to(&request, t.announce).await?;

                let mut buffer = [0; 1024];
                socket.recv_from(&mut buffer).await?;
                let res = bincode::deserialize::<tracker::Response>(&buffer)?;
                println!("{res:?}");
            } else {
                let res = reqwest::get(request.url(&t.announce)).await?;
                let res: tracker::Response =
                    serde_bencode::from_bytes(&res.bytes().await?).context("parse response")?;

                for peer in res.peers.0 {
                    println!("{peer}");
                }
            }
        }
    }

    Ok(())
}
