use std::path::PathBuf;

use anyhow::Context;
use bittorrent_cli::{
    torrent::{Keys, Torrent},
    tracker, udp,
};
use clap::{Parser, Subcommand};
use tokio::net::UdpSocket;

const MAX_PACKET_SIZE: usize = 1496;

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
                let socket = UdpSocket::bind("0.0.0.0:0").await?;
                socket.connect("tracker.opentracker.org:1337").await?;

                const CONNECT_ACTION: i32 = 0;
                const TRANSACTION_ID: i32 = 0;

                let mut connect_request = [0u8; 16];
                let protocol_id: i64 = 0x41727101980; // Magic constant
                let action: i32 = CONNECT_ACTION; // Action for connect is 0
                let transaction_id: i32 = TRANSACTION_ID;

                // Packing the connect request buffer
                connect_request[..8].copy_from_slice(&protocol_id.to_be_bytes());
                connect_request[8..12].copy_from_slice(&action.to_be_bytes());
                connect_request[12..].copy_from_slice(&transaction_id.to_be_bytes());

                // Send the connect request
                socket.send(&connect_request).await?;

                // Buffer to receive the response
                let mut response = [0u8; 16];

                // Set a timeout for the response
                // socket.set(Some(Duration::from_secs(5)))?;

                // Receive the response
                match socket.recv(&mut response).await {
                    Ok(_) => {
                        // Extract the action and transaction_id from the response
                        let action = i32::from_be_bytes(response[0..4].try_into().unwrap());
                        let res_transaction_id =
                            i32::from_be_bytes(response[4..8].try_into().unwrap());

                        // Check if the transaction_id matches
                        if res_transaction_id != transaction_id {
                            println!("Transaction ID does not match");
                            return Ok(());
                        }

                        if action == 0 {
                            // Success, extract connection_id
                            let connection_id =
                                i64::from_be_bytes(response[8..16].try_into().unwrap());
                            println!("Received connection ID: {}", connection_id);
                        } else {
                            // Error or unknown action
                            println!("Received unknown action: {}", action);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to receive response: {:?}", e);
                    }
                }
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
