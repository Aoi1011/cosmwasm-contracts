use std::net::SocketAddrV4;

use crate::{
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
                let piece = download_worker(&t, shared_state.clone(), &mut connection).await;
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

    Ok(Downloaded {
        bytes: todo!(),
        files: todo!(),
    })
}

pub async fn download_piece(
    candidate_peers: &[SocketAddrV4],
    piece_hash: [u8; 20],
    piece_size: usize,
) {
}

pub async fn download_block(peer: &SocketAddrV4, block: [u8; 20], block_size: usize) {}

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
