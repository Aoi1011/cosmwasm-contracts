use std::{io, net::SocketAddrV4, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};
use bittorrent_cli::{
    block,
    peer::{Handshake, Message, MessageId, Peer},
    torrent::{Keys, Torrent},
    tracker,
};
use clap::{Parser, Subcommand};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
};

const BLOCK_SIZE: u32 = 1 << 14;
const MAX_PACKET_SIZE: usize = 1496;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[clap(rename_all = "snake_case")]
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
    Handshake {
        torrent: PathBuf,
        addr: SocketAddrV4,
    },
    DownloadPiece {
        #[clap(short, long)]
        output: PathBuf,
        torrent: PathBuf,
        piece_index: u32,
    },
    Download {
        #[clap(short, long)]
        output: PathBuf,
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

            let addr = bittorrent_cli::tracker::get_addr(&t.announce)?;

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
                            // Connect
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

                            // Announce
                            1 => {
                                let mut announce_buffer = Vec::new();
                                transaction_id = rand::random::<u32>();
                                let announce_req = tracker::udp::AnnounceRequest::new(
                                    connection_id,
                                    transaction_id,
                                    t.info_hash(),
                                );
                                let request = tracker::udp::Request::from(announce_req);
                                request.write(&mut announce_buffer)?;

                                let mut attempts = 0;
                                let max_retries = 8;
                                let mut delay = 15;
                                loop {
                                    eprintln!("attempting to send request: {}", attempts);

                                    if attempts > max_retries {
                                        return Err(anyhow!("max retransmission reached"));
                                    }
                                    // Send the connect request
                                    match socket.send_to(&announce_buffer, &url).await {
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
                                    tracker::udp::Response::Announce(announce_res) => {
                                        assert_eq!(announce_res.transaction_id.0, transaction_id);

                                        // action = 1;
                                        // connection_id = announce_res.connection_id.0;

                                        eprintln!("Peers");
                                        for (idx, peer) in announce_res.peers.iter().enumerate() {
                                            eprintln!("Peer {idx}: {peer}");
                                        }

                                        break 'transmit;
                                    }
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
        Commands::Handshake { torrent, addr } => {
            let t = Torrent::new(torrent)?;
            let info_hash = t.info_hash();

            let mut stream = TcpStream::connect(addr).await?;

            let handshake = Handshake::new(&info_hash);
            let mut handshake_bytes = handshake.bytes();
            stream.write_all(&mut handshake_bytes).await?;

            let mut buffer = [0; 68];
            let mut total_read = 0;
            while total_read < buffer.len() {
                let read = stream.read(&mut buffer[total_read..]).await?;
                if read == 0 {
                    return Err(anyhow!("Connection closed by peer"));
                }
                total_read += read;
            }

            let handshake_res = Handshake::from_bytes(&buffer);

            println!("Peer ID: {}", hex::encode(handshake_res.peer_id));
        }
        Commands::DownloadPiece {
            output,
            torrent,
            piece_index,
        } => {
            let t = Torrent::new(torrent)?;
            let info_hash = t.info_hash();
            let request = tracker::http::Request::new(&info_hash, t.length());

            let addr = bittorrent_cli::tracker::get_addr(&t.announce)?;

            match addr {
                tracker::Addr::Udp(_url) => {}
                tracker::Addr::Http(url) => {
                    let res = reqwest::get(request.url(&url.to_string())).await?;
                    let tracker_res: tracker::http::Response =
                        serde_bencode::from_bytes(&res.bytes().await?).context("parse response")?;

                    let addr = tracker_res.peers.0[0];
                    let mut stream = TcpStream::connect(addr).await?;

                    let handshake = Handshake::new(&info_hash);
                    let mut handshake_bytes = handshake.bytes();
                    stream.write_all(&mut handshake_bytes).await?;

                    let mut buffer = [0; 68];
                    let mut total_read = 0;
                    while total_read < buffer.len() {
                        let read = stream.read(&mut buffer[total_read..]).await?;
                        if read == 0 {
                            return Err(anyhow!("Connection closed by peer"));
                        }
                        total_read += read;
                    }

                    let _handshake_res = Handshake::from_bytes(&buffer);

                    let _bitfield = Message::decode(&mut stream).await?;

                    Message::encode(&mut stream, MessageId::Interested, &mut []).await?;

                    let _unchoke = Message::decode(&mut stream).await?;

                    let plength = t.info.plength as u32;
                    let piece_length = plength.min(t.length() as u32 - plength * piece_index);
                    // let total_blocks = if piece_length % BLOCK_SIZE == 0 {
                    //     piece_length / BLOCK_SIZE
                    // } else {
                    //     (piece_length / BLOCK_SIZE) + 1
                    // };
                    let block_size = 2_u32.pow(14);
                    let mut remaining_piece = piece_length;

                    let _ = tokio::fs::remove_file(&output).await;
                    let mut output_file = tokio::fs::File::options()
                        .write(true)
                        .create(true)
                        .open(&output)
                        .await
                        .unwrap();

                    while remaining_piece > 0 {
                        let begin = piece_length - remaining_piece;
                        let block_size = std::cmp::min(block_size, remaining_piece);
                        remaining_piece -= block_size;

                        let block_req = block::Request {
                            piece_index,
                            begin,
                            length: block_size,
                        };
                        let mut block_payload = block_req.encode();

                        Message::encode(&mut stream, MessageId::Request, &mut block_payload)
                            .await?;

                        let piece = Message::decode(&mut stream).await?;
                        let payload_len = piece.payload.len();
                        let mut payload = io::Cursor::new(piece.payload);

                        let block_res = block::Response::new(&mut payload, payload_len).await?;

                        output_file.write_all(&block_res.block()).await?;
                    }

                    println!("Piece {piece_index} downloaded to {}", output.display());
                }
            }
        }
        Commands::Download { output, torrent } => {
            let t = Torrent::new(torrent)?;
            let info_hash = t.info_hash();
            let req = tracker::http::Request::new(&info_hash, t.length());
            let url = req.url(&t.announce);

            let res = reqwest::get(url).await?;
            let res_bytes = res.bytes().await?;
            let tracker_res: tracker::http::Response = serde_bencode::from_bytes(&res_bytes)?;

            let shared_state = Arc::new(SharedState::new(t.info.pieces.0.len()));

            let futures = tracker_res
                .peers
                .0
                .into_iter()
                .map(|addr| {
                    let info_hash = info_hash.clone();
                    async move { Peer::new(addr, &info_hash).await }
                })
                .collect::<Vec<_>>();

            let results = join_all(futures).await;
            for result in results {
                match result {
                    Ok(mut connection) => {
                        let piece =
                            download_worker(&t, shared_state.clone(), &mut connection).await;
                    }
                    Err(e) => eprintln!("Error: {}", e),
                };
            }

            let all_pieces = shared_state.all_pieces();
            let mut output_file = tokio::fs::File::options()
                .write(true)
                .create(true)
                .open(&output)
                .await
                .unwrap();
            output_file.write_all(&all_pieces).await?;

            println!("Downloaded test.torrent to {}.", output.display());
        }
    }

    Ok(())
}
