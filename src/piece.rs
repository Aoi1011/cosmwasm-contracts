use std::collections::HashSet;

use crate::{peer::Peer, torrent::Torrent};

#[derive(Debug, PartialEq, Eq)]
pub struct Piece {
    peers: HashSet<usize>,
    piece_i: usize,
    length: usize,
    hash: [u8; 20],
}

impl Ord for Piece {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.peers
            .len()
            .cmp(&other.peers.len())
            .then(self.peers.iter().cmp(other.peers.iter()))
            .then(self.hash.cmp(&other.hash))
            .then(self.length.cmp(&other.length))
            .then(self.piece_i.cmp(&other.piece_i))
    }
}

impl PartialOrd for Piece {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Piece {
    pub(crate) fn new(piece_i: usize, t: &Torrent, peers: &[Peer]) -> Self {
        let piece_hash = t.info.pieces.0[piece_i];
        let plength = t.info.plength;
        let piece_size = plength.min(t.length() - plength * piece_i);

        let peers = peers
            .iter()
            .enumerate()
            .filter_map(|(peer_i, peer)| peer.has_piece(piece_i).then_some(peer_i))
            .collect();

        Self {
            peers,
            piece_i,
            length: piece_size,
            hash: piece_hash,
        }
    }

    pub(crate) fn peers(&self) -> &HashSet<usize> {
        &self.peers
    }

    pub(crate) fn index(&self) -> usize {
        self.piece_i
    }

    pub(crate) fn length(&self) -> usize {
        self.length
    }

    pub(crate) fn hash(&self) -> &[u8] {
        &self.hash
    }
}
