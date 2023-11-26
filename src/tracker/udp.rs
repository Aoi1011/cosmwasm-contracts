use std::io::{self, Write};

use byteorder::{NetworkEndian, WriteBytesExt};
use serde::Deserialize;

const PROTOCOL_IDENTIFIER: u64 = 0x0417_2710_1980;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Deserialize)]
pub struct TransactionId(pub u32);

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Deserialize)]
pub struct ConnectionId(pub u64);

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct ConnectRequest {
    pub transaction_id: TransactionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Connect(ConnectRequest),
}

impl From<ConnectRequest> for Request {
    fn from(value: ConnectRequest) -> Self {
        Self::Connect(value)
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
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Deserialize)]
pub struct ConnectResponse {
    pub connection_id: ConnectionId,
    pub transaction_id: TransactionId,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Response {
    Connect(ConnectResponse),
}

impl Response {
    pub fn read(bytes: &[u8]) -> Result<Self, io::Error> {
        // let action = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let action = u32::from_be_bytes(bytes[0..4].try_into().unwrap());

        match action {
            // Connect
            0 => {
                let connection_id =
                    ConnectionId(u64::from_be_bytes(bytes[8..16].try_into().unwrap()));
                let transaction_id =
                    TransactionId(u32::from_be_bytes(bytes[4..8].try_into().unwrap()));

                Ok(Self::Connect(ConnectResponse {
                    connection_id,
                    transaction_id,
                }))
            }
            op => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{op}"))),
        }
    }

    pub fn write(&self, bytes: &mut impl Write) -> Result<(), io::Error> {
        match self {
            Response::Connect(r) => {
                bytes.write_u32::<NetworkEndian>(0)?;
                bytes.write_u32::<NetworkEndian>(r.transaction_id.0)?;
                bytes.write_u64::<NetworkEndian>(r.connection_id.0)?;
            }
        }

        Ok(())
    }
}
