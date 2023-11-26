use std::{net::UdpSocket, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context};
use bittorrent_cli::{
    torrent::{Keys, Torrent},
    tracker::{self, udp::TransactionId},
};
use clap::{Parser, Subcommand};
use tokio::net::unix::SocketAddr;
// use tokio::net::UdpSocket;

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
        Commands::Peers { torrent } => {
            let t_bytes = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent = serde_bencode::from_bytes(&t_bytes).context("parse torrent file")?;

            let file_length = match t.info.keys {
                Keys::SingleFile { length } => length,
                Keys::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
            };
            println!("Tracker URL: {}", t.announce);
            let info_hash = t.info_hash();
            let request = tracker::http::Request::new(&info_hash, file_length);

            let addr = bittorrent_cli::tracker::get_addr(t.announce)?;

            match addr {
                bittorrent_cli::tracker::Addr::Udp(url) => {
                    let socket = UdpSocket::bind("0.0.0.0:0").context("")?;
                    socket.connect(url).context("connect to tracker")?;

                    const CONNECT_ACTION: u32 = 0;
                    let transaction_id: u32 = rand::random();

                    let mut connect_buffer = Vec::new();
                    // let protocol_id: u64 = 0x0417_2710_1980; // Magic constant
                    // let action: u32 = CONNECT_ACTION; // Action for connect is 0
                    let transaction_id: u32 = transaction_id;
                    let connect_req = tracker::udp::ConnectRequest {
                        transaction_id: TransactionId(transaction_id),
                    };
                    let request = tracker::udp::Request::from(connect_req);

                    // Packing the connect request buffer
                    // connect_request[..8].copy_from_slice(&protocol_id.to_be_bytes());
                    // connect_request[8..12].copy_from_slice(&action.to_be_bytes());
                    // connect_request[12..].copy_from_slice(&transaction_id.to_be_bytes());
                    request.write(&mut connect_buffer)?;

                    let mut attempts = 0;
                    let max_retries = 8;
                    let mut delay = 15;
                    loop {
                        eprintln!("attempting to send request: {}", attempts);

                        if attempts > max_retries {
                            // return Err(io::Error::new(io::ErrorKind::Other, "max retransmission reached"));
                            return Err(anyhow!("max retransmission reached"));
                        }
                        // Send the connect request
                        match socket.send_to(&connect_buffer, &url) {
                            Ok(_) => break,
                            Err(e) => {
                                println!(
                                    "attempt {}: Failed to send request, error: {}",
                                    attempts, e
                                );
                            }
                        }

                        tokio::time::sleep(Duration::from_secs(delay)).await;

                        attempts += 1;

                        delay *= 2;
                    }

                    // Buffer to receive the response
                    let mut response = [0u8; 16];

                    // Set a timeout for the response
                    // socket.set(Some(Duration::from_secs(5)))?;

                    // Receive the response
                    match socket.recv(&mut response) {
                        Ok(_) => {
                            // Extract the action and transaction_id from the response
                            let action = i32::from_be_bytes(response[0..4].try_into().unwrap());
                            let res_transaction_id =
                                u32::from_be_bytes(response[4..8].try_into().unwrap());

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
                }
                bittorrent_cli::tracker::Addr::Http(url) => {
                    let res = reqwest::get(request.url(&url.to_string())).await?;
                    let res: tracker::http::Response =
                        serde_bencode::from_bytes(&res.bytes().await?).context("parse response")?;

                    for peer in res.peers.0 {
                        println!("{peer}");
                    }
                }
            }
        }
    }

    Ok(())
}
