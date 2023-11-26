use std::io::{self, Cursor, Write};

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

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Response {
    Connect(ConnectResponse),
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
