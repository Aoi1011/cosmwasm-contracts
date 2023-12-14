use std::{collections::BinaryHeap, time::Duration};

use anyhow::{anyhow, Context};
use futures_util::StreamExt;
use sha1::{Digest, Sha1};
use tokio::net::UdpSocket;

use crate::{
    block::BLOCK_SIZE,
    peer::Peer,
    piece::Piece,
    torrent::{File, Keys, Torrent},
    tracker,
};

pub async fn all(t: &Torrent) -> anyhow::Result<Downloaded> {
    let info_hash = t.info_hash();
    let request = tracker::http::Request::new(&info_hash, t.length());
    let addr = tracker::get_addr(&t.announce)?;

    let peers = match addr {
        tracker::Addr::Udp(url) => {
            let socket = UdpSocket::bind("0.0.0.0:0")
                .await
                .context("bind to the address")?;
            socket.connect(url).await.context("connect to tracker")?;

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
                        let res =
                            tracker::udp::Response::read(&mut response).context("read response")?;

                        // Check if the transaction_id matches
                        match res {
                            tracker::udp::Response::Connect(connect_res) => {
                                assert_eq!(connect_res.transaction_id.0, transaction_id);

                                println!("Received connection ID: {}", connect_res.connection_id.0);

                                action = 1;
                                connection_id = connect_res.connection_id.0;
                            }
                            tracker::udp::Response::Announce(announce_res) => {
                                assert_eq!(announce_res.transaction_id.0, transaction_id);

                                eprintln!("Peers");

                                break announce_res.peers;
                                // break 'transmit;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to receive response: {:?}", e);
                    }
                }
            }
        }
        tracker::Addr::Http(url) => {
            let res = reqwest::get(request.url(&url.to_string())).await?;
            let res: tracker::http::Response =
                serde_bencode::from_bytes(&res.bytes().await?).context("parse response")?;

            res.peers.0
        }
    };

    let mut peers = futures_util::stream::iter(peers)
        .map(|peer_addr| async move {
            let peer = Peer::new(peer_addr, &info_hash).await;
            (peer_addr, peer)
        })
        .buffer_unordered(5);

    let mut peer_list = Vec::new();
    while let Some((peer_addr, peer)) = peers.next().await {
        match peer {
            Ok(peer) => {
                peer_list.push(peer);

                if peer_list.len() > 5 {
                    break;
                }
            }
            Err(e) => {
                eprintln!("fail to connect to peer {peer_addr:?}: {e}");
            }
        }
    }
    drop(peers);

    let mut peers = peer_list;
    let mut need_pieces = BinaryHeap::new();
    let mut no_peers = Vec::new();

    for piece_i in 0..t.info.pieces.0.len() {
        let piece = Piece::new(piece_i, &t, &peers);
        if piece.peers().is_empty() {
            no_peers.push(piece);
        } else {
            need_pieces.push(piece);
        }
    }

    assert!(no_peers.is_empty());

    let mut all_pieces = vec![0; t.length()];
    while let Some(piece) = need_pieces.pop() {
        let plength = piece.length();
        let npiece = piece.index();
        let piece_length = plength.min(t.length() - plength * npiece);
        let total_blocks = if piece_length % BLOCK_SIZE as usize == 0 {
            piece_length / BLOCK_SIZE as usize
        } else {
            (piece_length / BLOCK_SIZE as usize) + 1
        };

        let peers: Vec<_> = peers
            .iter_mut()
            .enumerate()
            .filter_map(|(peer_i, peer)| piece.peers().contains(&peer_i).then_some(peer))
            .collect();

        let (submit, tasks) = kanal::bounded_async(total_blocks);
        for block in 0..total_blocks {
            submit
                .send(block)
                .await
                .expect("bound holds all these limits");
        }

        let (finish, mut done) = tokio::sync::mpsc::channel(total_blocks);
        let mut participants = futures_util::stream::FuturesUnordered::new();
        for peer in peers {
            participants.push(peer.participate(
                piece.index() as u32,
                total_blocks as u32,
                piece_length as u32,
                submit.clone(),
                tasks.clone(),
                finish.clone(),
            ));
        }
        drop(submit);
        drop(finish);
        drop(tasks);

        let mut all_blocks: Vec<u8> = vec![0; piece_length];
        let mut bytes_received = 0;
        loop {
            tokio::select! {
                joined = participants.next(), if !participants.is_empty() => {
                    // if a participant ends early, it's either slow or failed.
                    match joined {
                        None => {},
                        Some(Ok(_)) => {},
                        Some(Err(_)) => {},
                    }
                },

                piece = done.recv() => {
                // keep track of the bytes in message
                    if let Some(piece) = piece {
                        // let piece = Piece::ref_from_bytes(&piece.block()[..]).expect("always get all Piece response fields from peer");
                        all_blocks[piece.begin() as usize ..][..piece.block().len()].copy_from_slice(piece.block());
                        bytes_received += piece.block().len();
                        if bytes_received ==  piece_length {
                            break;
                        }
                    } else {
                        break;
                    }

                },
            }
        }
        drop(participants);

        if bytes_received == piece_length {
            // great, we got all the bytes
        } else {
            // we'll need to connect to more peers, and make sure that those additional peers also
            // have this piece, and then download the piece we _didn't_ get from them.
            // probably also stick this back onto the pices_heap
            anyhow::bail!("no peers left to get piece {}", piece.index());
        }

        let mut hasher = Sha1::new();
        hasher.update(&all_blocks);
        let hash: [u8; 20] = hasher.finalize().try_into().expect("");
        assert_eq!(hash, piece.hash());

        all_pieces[piece.index() * t.info.plength..][..piece_length].copy_from_slice(&all_blocks);
    }

    Ok(Downloaded {
        bytes: all_pieces,
        files: match &t.info.keys {
            Keys::SingleFile { length } => vec![File {
                length: *length,
                path: vec![t.info.name.clone()],
            }],
            Keys::MultiFile { files } => files.clone(),
        },
    })
}

pub struct Downloaded {
    pub bytes: Vec<u8>,
    pub files: Vec<File>,
}

impl<'a> IntoIterator for &'a Downloaded {
    type Item = DownloadedFile<'a>;
    type IntoIter = DownloadedIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DownloadedIter::new(self)
    }
}

pub struct DownloadedIter<'d> {
    downloaded: &'d Downloaded,
    file_iter: std::slice::Iter<'d, File>,
    offset: usize,
}

impl<'d> DownloadedIter<'d> {
    pub fn new(d: &'d Downloaded) -> Self {
        Self {
            downloaded: d,
            file_iter: d.files.iter(),
            offset: 0,
        }
    }
}

impl<'d> Iterator for DownloadedIter<'d> {
    type Item = DownloadedFile<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file_iter.next()?;
        let bytes = &self.downloaded.bytes[self.offset..][..file.length];
        Some(DownloadedFile { file, bytes })
    }
}

pub struct DownloadedFile<'d> {
    file: &'d File,
    bytes: &'d [u8],
}

impl<'d> DownloadedFile<'d> {
    pub fn path(&self) -> &'d [String] {
        &self.file.path
    }

    pub fn bytes(&self) -> &'d [u8] {
        self.bytes
    }
}
