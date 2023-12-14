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
            peers: Peers(Vec::new()),
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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use actix_web::{test, web, App, HttpResponse, Responder};

    use crate::{
        torrent::{Hashes, Info, Keys, Torrent},
        tracker,
    };

    #[test]
    async fn test_build_tracker_url() {
        let t = Torrent {
            announce: "http://bttracker.debian.org:6969/announce".to_string(),
            info: Info {
                name: "debian-10.2.0-amd64-netinst.iso".to_string(),
                plength: 262144,
                pieces: Hashes(vec![
                    [
                        49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 97, 98, 99, 100, 101, 102, 103,
                        104, 105, 106,
                    ],
                    [
                        97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 49, 50, 51, 52, 53, 54, 55,
                        56, 57, 48,
                    ],
                ]),
                keys: Keys::SingleFile { length: 351272960 },
            },
        };

        let info_hash = vec![
            216, 247, 57, 206, 195, 40, 149, 108, 204, 91, 191, 31, 134, 217, 253, 207, 219, 168,
            206, 182,
        ];
        let _peer_id = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        let _port = 6882_u16;
        let length = t.length();

        let tracker_req = tracker::http::Request::new(&info_hash, length);

        assert_eq!(tracker_req.url(&t.announce), "http://bttracker.debian.org:6969/announce?info_hash=%D8%F79%CE%C3%28%95l%CC%5B%BF%1F%86%D9%FD%CF%DB%A8%CE%B6&peer_id=00112233445566778899&port=6881&uploaded=0&downloaded=0&left=351272960&compact=1");
    }

    async fn mock_response() -> impl Responder {
        let mut res_body: Vec<u8> = Vec::new();

        res_body.extend(b"d8:intervali900e5:peers12:");
        res_body.extend([192, 0, 2, 123, 0x1A, 0xE1]);
        res_body.extend([127, 0, 0, 1, 0x1A, 0xE9]);
        res_body.extend(b"e");

        HttpResponse::Ok().body(res_body)
    }

    #[actix_rt::test]
    async fn test_request_peers() {
        let mut app = test::init_service(App::new().route("/", web::get().to(mock_response))).await;

        let req = test::TestRequest::get().uri("/").to_request();
        let res = test::call_service(&mut app, req).await;
        let result = test::read_body(res).await;

        eprintln!("{:?}", result);

        let tracker_res: tracker::http::Response = serde_bencode::from_bytes(&result).unwrap();

        let expected = vec![
            SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 123), 6881),
            SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 6889),
        ];

        assert_eq!(tracker_res.peers.0, expected);
    }
}
