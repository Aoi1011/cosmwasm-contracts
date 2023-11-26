use std::{io::Cursor, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context};
use bittorrent_cli::{
    torrent::{Keys, Torrent},
    tracker::{self, udp::TransactionId},
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
                    let socket = UdpSocket::bind("0.0.0.0:0")
                        .await
                        .context("bind to the address")?;
                    socket.connect(url).await.context("connect to tracker")?;

                    const CONNECT_ACTION: u32 = 0;
                    let mut action = 0;
                    let mut transaction_id = 0;
                    let mut connection_id: u64 = 0;

                    'transmit: loop {
                        match action {
                            0 => {
                                let mut connect_buffer = Vec::new();
                                transaction_id = rand::random::<u32>();
                                let connect_req = tracker::udp::ConnectRequest::new(transaction_id);
                                let request = tracker::udp::Request::from(connect_req);
                                request.write(&mut connect_buffer)?;

                                let mut attempts = 0;
                                let max_retries = 8;
                                let mut delay = 15;
                                loop {
                                    eprintln!("attempting to send request: {}", attempts);

                                    if attempts > max_retries {
                                        return Err(anyhow!("max retransmission reached"));
                                    }
                                    // Send the connect request
                                    match socket.send_to(&connect_buffer, &url).await {
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
                            }
                            1 => {
                                let mut connect_buffer = Vec::new();
                                transaction_id = rand::random::<u32>();
                                let connect_req = tracker::udp::ConnectRequest::new(transaction_id);
                                let request = tracker::udp::Request::from(connect_req);
                                request.write(&mut connect_buffer)?;

                                let mut attempts = 0;
                                let max_retries = 8;
                                let mut delay = 15;
                                loop {
                                    eprintln!("attempting to send request: {}", attempts);

                                    if attempts > max_retries {
                                        return Err(anyhow!("max retransmission reached"));
                                    }
                                    // Send the connect request
                                    match socket.send_to(&connect_buffer, &url).await {
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
                                break 'transmit;
                            }
                            _ => {}
                        }

                        // Buffer to receive the response
                        let mut response: Vec<u8> = vec![0; 1206];

                        // Receive the response
                        match socket.recv(&mut response).await {
                            Ok(_) => {
                                let res = tracker::udp::Response::read(&mut response)
                                    .context("read response")?;

                                // Check if the transaction_id matches
                                match res {
                                    tracker::udp::Response::Connect(connect_res) => {
                                        assert_eq!(connect_res.transaction_id.0, transaction_id);

                                        println!(
                                            "Received connection ID: {}",
                                            connect_res.connection_id.0
                                        );

                                        action = 1;
                                        connection_id = connect_res.connection_id.0;
                                    }
                                    tracker::udp::Response::Announce(announce_res) => {}
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to receive response: {:?}", e);
                            }
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
