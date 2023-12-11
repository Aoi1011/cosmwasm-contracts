use std::{collections::BinaryHeap, net::SocketAddrV4};

use futures_util::StreamExt;

use crate::{
    block::BLOCK_SIZE,
    peer::Peer,
    piece::Piece,
    torrent::{File, Torrent},
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

    let all_pieces = vec![0; t.length()];
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
            .filter_map(|(peer_i, peer)| piece.peers().contains(&peer_i).then_some(peer));

        let (submit, tasks) = kanal
    }
    let mut output_file = tokio::fs::File::options()
        .write(true)
        .create(true)
        .open(&output)
        .await
        .unwrap();
    output_file.write_all(&all_pieces).await?;

    Ok(Downloaded {
        bytes: todo!(),
        files: todo!(),
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
