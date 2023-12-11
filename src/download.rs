use std::collections::BinaryHeap;

use futures_util::StreamExt;
use sha1::{Digest, Sha1};

use crate::{
    block::BLOCK_SIZE,
    peer::Peer,
    piece::Piece,
    torrent::{File, Keys, Torrent},
    tracker,
};

pub async fn all(t: &Torrent) -> anyhow::Result<Downloaded> {
    let info_hash = t.info_hash();
    let req = tracker::http::Request::new(&info_hash, t.length());
    let url = req.url(&t.announce);

    let res = reqwest::get(url).await?;
    let res_bytes = res.bytes().await?;
    let tracker_res: tracker::http::Response = serde_bencode::from_bytes(&res_bytes)?;

    let mut peers = futures_util::stream::iter(tracker_res.peers.0)
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
                eprintln!("connect to peer {peer_addr:?}: {e}");
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
    bytes: Vec<u8>,
    files: Vec<File>,
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
