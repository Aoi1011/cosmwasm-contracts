use anyhow::Context;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone)]
pub struct Handshake {
    pub length: u8,
    pub protocol: Vec<u8>,
    pub reserved: Vec<u8>,
    pub info_hash: Vec<u8>,
    pub peer_id: Vec<u8>,
}

impl Handshake {
    pub fn new(info_hash: &[u8; 20]) -> Self {
        Self {
            length: 19,
            protocol: b"BitTorrent protocol".to_vec(),
            reserved: vec![0; 8],
            info_hash: info_hash.to_vec(),
            peer_id: b"00112233445566778899".to_vec(),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            length: bytes[0],
            protocol: bytes[1..20].to_vec(),
            reserved: bytes[20..28].to_vec(),
            info_hash: bytes[28..48].to_vec(),
            peer_id: bytes[48..].to_vec(),
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(68);

        bytes.push(self.length);
        bytes.extend(self.protocol.clone());
        bytes.extend(self.reserved.clone());
        bytes.extend(self.info_hash.clone());
        bytes.extend(self.peer_id.clone());

        bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageId {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
    Error,
}

impl From<u8> for MessageId {
    fn from(value: u8) -> Self {
        match value {
            0 => MessageId::Choke,
            1 => MessageId::Unchoke,
            2 => MessageId::Interested,
            3 => MessageId::NotInterested,
            4 => MessageId::Have,
            5 => MessageId::Bitfield,
            6 => MessageId::Request,
            7 => MessageId::Piece,
            8 => MessageId::Cancel,
            _ => MessageId::Error,
        }
    }
}

impl From<MessageId> for u8 {
    fn from(value: MessageId) -> Self {
        match value {
            MessageId::Choke => 0,
            MessageId::Unchoke => 1,
            MessageId::Interested => 2,
            MessageId::NotInterested => 3,
            MessageId::Have => 4,
            MessageId::Bitfield => 5,
            MessageId::Request => 6,
            MessageId::Piece => 7,
            MessageId::Cancel => 8,
            MessageId::Error => panic!(),
        }
    }
}

pub struct Message {
    pub length: u32,
    pub id: MessageId,
    pub payload: Vec<u8>,
}

impl Message {
    pub async fn decode<R>(buf: &mut R) -> anyhow::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        eprintln!("got a response");
        let length = buf.read_u32().await.context("can not read length u32")?;
        eprintln!("Length: {length}");
        let id = buf.read_u8().await.context("can not id length u32")?;
        eprintln!("id: {id}");
        let mut payload = vec![0; (length - 1) as usize];
        buf.read_exact(&mut payload).await?;

        Ok(Self {
            length,
            id: MessageId::from(id),
            payload,
        })
    }

    pub async fn encode<W>(w: &mut W, id: MessageId, payload: &mut [u8]) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let len_buf = (payload.len() + 1) as u32;

        w.write_u32_le(len_buf).await?;
        w.write_u8(id.into()).await?;
        w.write_all(payload).await?;
        w.flush().await?;

        Ok(())
    }
}
