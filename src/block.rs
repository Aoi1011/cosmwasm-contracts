use tokio::io::{AsyncRead, AsyncReadExt};

pub(crate) const BLOCK_SIZE: u32 = 1 << 14;

#[derive(Debug, Clone)]
pub struct Request {
    pub piece_index: u32,
    pub begin: u32,
    pub length: u32,
}

impl Request {
    pub fn new(piece_index: u32, remaining_piece: u32, plength: u32) -> Self {
        let begin = plength - remaining_piece;
        let block_size = std::cmp::min(BLOCK_SIZE, remaining_piece);

        Self {
            piece_index,
            begin,
            length: block_size,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::new();

        payload.extend(u32::to_be_bytes(self.piece_index));
        payload.extend(u32::to_be_bytes(self.begin));
        payload.extend(u32::to_be_bytes(self.length));

        payload
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    index: u32,
    begin: u32,
    block: Vec<u8>,
}

impl Response {
    pub async fn new<R>(buf: &mut R, payload_length: usize) -> anyhow::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let index = buf.read_u32().await?;
        let begin = buf.read_u32().await?;

        let block_len = payload_length - 4 - 4;
        let mut block = vec![0; block_len];
        buf.read_exact(&mut block).await?;

        Ok(Self {
            index,
            begin,
            block,
        })
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn block(&self) -> &[u8] {
        &self.block
    }
}
