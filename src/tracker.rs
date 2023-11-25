use std::{
    fmt,
    net::{Ipv4Addr, SocketAddrV4},
};

use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};

#[derive(Debug, Clone, Serialize)]
pub struct Request<'caller> {
    pub info_hash: &'caller [u8],
    pub peer_id: &'caller [u8],
    pub port: u16,
    pub uploaded: usize,
    pub downloaded: usize,
    pub left: usize,
    pub compact: u8,
}

impl<'a> Request<'a> {
    pub fn new(info_hash: &'a [u8], left: usize) -> Self {
        Self {
            info_hash,
            peer_id: b"00112233445566778899",
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left,
            compact: 1,
        }
    }

    pub fn url(&'a self, announce: &str) -> String {
        let url_encoded_info_hash = urlencoding::encode_binary(self.info_hash);
        let url_encoded_peer_id = urlencoding::encode_binary(self.peer_id);

        let mut url = String::new();
        url.push_str(announce);
        url.push('?');
        url.push_str("info_hash=");
        url.push_str(&url_encoded_info_hash);
        url.push('&');
        url.push_str("peer_id=");
        url.push_str(&url_encoded_peer_id);
        url.push('&');
        url.push_str("port=");
        url.push_str(&self.port.to_string());
        url.push('&');
        url.push_str("uploaded=");
        url.push_str(&self.uploaded.to_string());
        url.push('&');
        url.push_str("downloaded=");
        url.push_str(&self.downloaded.to_string());
        url.push('&');
        url.push_str("left=");
        url.push_str(&self.left.to_string());
        url.push('&');
        url.push_str("compact=");
        url.push_str(&(self.compact as u8).to_string());

        url
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub interval: u16,
    pub peers: Peers,
}

impl Response {
    pub fn new() -> Self {
        Self {
            interval: 0,
            peers: Peers (Vec::new()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Peers(pub Vec<SocketAddrV4>);
struct PeersVisitor;

impl<'de> Visitor<'de> for PeersVisitor {
    type Value = Peers;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a byte string whose length is multiple of 6")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v.len() % 6 != 0 {
            return Err(E::custom(format!("length is {}", v.len())));
        }

        Ok(Peers(
            v.chunks_exact(6)
                .map(|slice_6| {
                    SocketAddrV4::new(
                        Ipv4Addr::new(slice_6[0], slice_6[1], slice_6[2], slice_6[3]),
                        u16::from_be_bytes([slice_6[4], slice_6[5]]),
                    )
                })
                .collect(),
        ))
    }
}

impl<'de> Deserialize<'de> for Peers {
    fn deserialize<D>(deserializer: D) -> Result<Peers, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(PeersVisitor)
    }
}

impl Serialize for Peers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut single_slice = Vec::with_capacity(6 * self.0.len());
        for peer in &self.0 {
            single_slice.extend_from_slice(&peer.ip().octets());
            single_slice.extend_from_slice(&peer.port().to_be_bytes());
        }
        serializer.serialize_bytes(&single_slice)
    }
}
