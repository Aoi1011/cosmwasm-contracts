use std::{
    io::{self, Cursor, Read, Write},
    net::{Ipv4Addr, SocketAddrV4},
};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use serde::Deserialize;

const PROTOCOL_IDENTIFIER: u64 = 0x0417_2710_1980;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Deserialize)]
pub struct TransactionId(pub u32);

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Deserialize)]
pub struct ConnectionId(pub u64);

/// Offset  Size            Name            Value
/// 0       64-bit integer  protocol_id     0x41727101980 // magic constant
/// 8       32-bit integer  action          0 // connect
/// 12      32-bit integer  transaction_id
/// 16
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct ConnectRequest {
    protocol_id: u64,
    action: u32,
    transaction_id: TransactionId,
}

impl ConnectRequest {
    pub fn new(transaction_id: u32) -> Self {
        Self {
            protocol_id: PROTOCOL_IDENTIFIER,
            action: 0,
            transaction_id: TransactionId(transaction_id),
        }
    }
}

/// Offset  Size    Name    Value
/// 0       64-bit integer  connection_id
/// 8       32-bit integer  action          1 // announce
/// 12      32-bit integer  transaction_id
/// 16      20-byte string  info_hash
/// 36      20-byte string  peer_id
/// 56      64-bit integer  downloaded
/// 64      64-bit integer  left
/// 72      64-bit integer  uploaded
/// 80      32-bit integer  event           0 // 0: none; 1: completed; 2: started; 3: stopped
/// 84      32-bit integer  IP address      0 // default
/// 88      32-bit integer  key
/// 92      32-bit integer  num_want        -1 // default
/// 96      16-bit integer  port
/// 98
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnnounceRequest {
    pub connection_id: ConnectionId,
    pub transaction_id: TransactionId,
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
    pub downloaded: u64,
    pub left: u64,
    pub uploaded: u64,
    pub event: u32,
    pub ip_address: u32,
    pub key: u32,
    pub num_want: i32,
    pub port: u16,
}

impl AnnounceRequest {
    pub fn new(connection_id: u64, transaction_id: u32, info_hash: [u8; 20]) -> Self {
        Self {
            connection_id: ConnectionId(connection_id),
            transaction_id: TransactionId(transaction_id),
            info_hash,
            peer_id: *b"00112233445566778899",
            downloaded: 0,
            left: 0,
            uploaded: 0,
            event: 0,
            ip_address: 0,
            key: 0,
            num_want: -1,
            port: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Connect(ConnectRequest),
    Announce(AnnounceRequest),
}

impl From<ConnectRequest> for Request {
    fn from(value: ConnectRequest) -> Self {
        Self::Connect(value)
    }
}

impl From<AnnounceRequest> for Request {
    fn from(value: AnnounceRequest) -> Self {
        Self::Announce(value)
    }
}

impl Request {
    pub fn write(self, bytes: &mut impl Write) -> Result<(), io::Error> {
        match self {
            Request::Connect(r) => {
                bytes.write_u64::<NetworkEndian>(PROTOCOL_IDENTIFIER)?;
                bytes.write_u32::<NetworkEndian>(0)?;
                bytes.write_u32::<NetworkEndian>(r.transaction_id.0)?;
            }
            Request::Announce(r) => {
                bytes.write_u64::<NetworkEndian>(r.connection_id.0)?;

                // announce action
                bytes.write_u32::<NetworkEndian>(1)?;
                bytes.write_u32::<NetworkEndian>(r.transaction_id.0)?;
                bytes.write_all(&r.info_hash[..])?;
                bytes.write_all(&r.peer_id[..])?;
                bytes.write_u64::<NetworkEndian>(r.downloaded)?;
                bytes.write_u64::<NetworkEndian>(r.left)?;
                bytes.write_u64::<NetworkEndian>(r.uploaded)?;
                bytes.write_u32::<NetworkEndian>(0)?;
                bytes.write_u32::<NetworkEndian>(0)?;
                bytes.write_u32::<NetworkEndian>(r.key)?;
                bytes.write_i32::<NetworkEndian>(r.num_want)?;
                bytes.write_u16::<NetworkEndian>(r.port)?;
            }
        }

        Ok(())
    }
}

/// Offset  Size            Name            Value
/// 0       32-bit integer  action          0 // connect
/// 4       32-bit integer  transaction_id
/// 8       64-bit integer  connection_id
/// 16
#[derive(PartialEq, Eq, Clone, Debug, Deserialize)]
pub struct ConnectResponse {
    pub connection_id: ConnectionId,
    pub transaction_id: TransactionId,
}

/// Offset      Size            Name            Value
/// 0           32-bit integer  action          1 // announce
/// 4           32-bit integer  transaction_id
/// 8           32-bit integer  interval
/// 12          32-bit integer  leechers
/// 16          32-bit integer  seeders
/// 20 + 6 * n  32-bit integer  IP address
/// 24 + 6 * n  16-bit integer  TCP port
/// 20 + 6 * N
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AnnounceResponse {
    pub transaction_id: TransactionId,
    pub interval: u32,
    pub leechers: u32,
    pub seeders: u32,
    pub peers: Vec<SocketAddrV4>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Response {
    Connect(ConnectResponse),
    Announce(AnnounceResponse),
}

impl Response {
    pub fn read(bytes: &[u8]) -> Result<Self, io::Error> {
        let mut cursor = Cursor::new(bytes);
        let action = cursor.read_u32::<NetworkEndian>()?;

        match action {
            // Connect
            0 => {
                let transaction_id = TransactionId(cursor.read_u32::<NetworkEndian>()?);
                let connection_id = ConnectionId(cursor.read_u64::<NetworkEndian>()?);

                Ok(Self::Connect(ConnectResponse {
                    connection_id,
                    transaction_id,
                }))
            }

            // Announce
            1 => {
                let transaction_id = TransactionId(cursor.read_u32::<NetworkEndian>()?);
                let interval = cursor.read_u32::<NetworkEndian>()?;
                let leechers = cursor.read_u32::<NetworkEndian>()?;
                let seeders = cursor.read_u32::<NetworkEndian>()?;
                let mut peers = Vec::new();
                loop {
                    let mut buf = [0; 6];
                    match cursor.read_exact(&mut buf) {
                        Ok(_) => {
                            let peer = SocketAddrV4::new(
                                Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]),
                                u16::from_be_bytes([buf[4], buf[5]]),
                            );
                            peers.push(peer);
                        }
                        Err(_) => break,
                    }
                }

                Ok(Self::Announce(AnnounceResponse {
                    transaction_id,
                    interval,
                    leechers,
                    seeders,
                    peers,
                }))
            }
            op => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{op}"))),
        }
    }

    // pub fn write(&self, bytes: &mut impl Write) -> Result<(), io::Error> {
    //     match self {
    //         Response::Connect(r) => {
    //             bytes.write_u32::<NetworkEndian>(0)?;
    //             bytes.write_u32::<NetworkEndian>(r.transaction_id.0)?;
    //             bytes.write_u64::<NetworkEndian>(r.connection_id.0)?;
    //         }
    //     }

    //     Ok(())
    // }
}
