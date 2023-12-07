use std::{io, net::SocketAddrV4};

use anyhow::Context;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

use crate::block::{self, BLOCK_SIZE};

#[derive(Debug, Clone)]
pub struct Handshake {
    pub length: u8,
    pub protocol: Vec<u8>,
    pub reserved: Vec<u8>,
    pub info_hash: Vec<u8>,
    pub peer_id: Vec<u8>,
}

pub struct Peer {
    addr: SocketAddrV4,
    stream: TcpStream,
}

impl Peer {
    pub async fn new(addr: SocketAddrV4, info_hash: &[u8; 20]) -> anyhow::Result<Self> {
        let mut stream = TcpStream::connect(addr).await.context("connect to peer")?;

        let handshake = Handshake::new(info_hash);
        {
            let mut handshake_bytes = handshake.bytes();
            stream.write_all(&mut handshake_bytes).await?;

            stream.read_exact(&mut handshake_bytes).await?;
        }

        anyhow::ensure!(handshake.length == 19);
        anyhow::ensure!(handshake.protocol == *b"BitTorrent protocol");

        let bitfield = Message::decode(&mut stream).await?;
        anyhow::ensure!(bitfield.id == MessageId::Bitfield);
        eprintln!("Received bitfield");

        Ok(Self { addr, stream })
    }

    pub(crate) async fn download_piece(
        &mut self,
        file_length: u32,
        npiece: u32,
        plength: u32,
    ) -> anyhow::Result<Vec<u8>> {
        eprintln!("start downloading piece: {npiece}, piece length: {plength}");
        // let mut buffer = [0; 68];
        // let mut total_read = 0;
        // while total_read < buffer.len() {
        //     let read = self.stream.read(&mut buffer[total_read..]).await?;
        //     if read == 0 {
        //         return Err(anyhow!("Connection closed by peer"));
        //     }
        //     total_read += read;
        // }
        // eprintln!("Total read: {total_read}");

        // let _handshake_res = Handshake::from_bytes(&buffer);

        Message::encode(&mut self.stream, MessageId::Interested, &mut []).await?;

        let unchoke = Message::decode(&mut self.stream).await?;
        anyhow::ensure!(unchoke.id == MessageId::Unchoke);
        eprintln!("Received unchoke");

        let mut all_pieces: Vec<u8> = Vec::new();
        let piece_length = plength.min(file_length - plength * npiece);
        let total_blocks = if piece_length % BLOCK_SIZE == 0 {
            piece_length / BLOCK_SIZE
        } else {
            (piece_length / BLOCK_SIZE) + 1
        };

        for nblock in 0..total_blocks {
            let block_req = block::Request::new(npiece as u32, nblock, piece_length);
            let mut block_payload = block_req.encode();

            Message::encode(&mut self.stream, MessageId::Request, &mut block_payload).await?;

            let piece = Message::decode(&mut self.stream).await?;
            let payload_len = piece.payload.len();
            let mut payload = io::Cursor::new(piece.payload);

            let block_res = block::Response::new(&mut payload, payload_len).await?;
            all_pieces.extend(block_res.block());
        }

        Ok(all_pieces)
    }
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
