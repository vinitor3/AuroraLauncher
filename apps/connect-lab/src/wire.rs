use crate::{Capability, Error, Protocol, Result, SCHEMA_VERSION};
use bytes::Bytes;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const DATAGRAM_HEADER_LEN: usize = 24;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Hello {
    pub schema_version: u8,
    pub session_id: String,
    pub lease_id: String,
    pub capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<InvitationProof>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InvitationProof {
    pub digest: String,
    pub generation: u64,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Operation {
    Open,
    OpenFragmented,
    CloseUdp,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OpenRequest {
    pub schema_version: u8,
    pub request_id: u64,
    pub service_id: String,
    pub service_revision: u32,
    pub protocol: Protocol,
    pub operation: Operation,
    pub association_id: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Reply {
    pub error: Option<Error>,
    pub service_handle: u32,
    pub association_id: u64,
    pub generation: u32,
    pub max_payload: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_version: Option<u8>,
}
impl Reply {
    pub fn error(error: Error) -> Self {
        Self {
            error: Some(error),
            ..Default::default()
        }
    }
    pub fn check(self) -> Result<Self> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(self),
        }
    }
}

pub(crate) async fn write_message<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    value: &T,
    max: usize,
) -> Result<()> {
    let data = serde_json::to_vec(value).map_err(|_| Error::Protocol)?;
    if data.len() > max {
        return Err(Error::Limit);
    }
    writer
        .write_u32(data.len() as u32)
        .await
        .map_err(|_| Error::Transport)?;
    writer.write_all(&data).await.map_err(|_| Error::Transport)
}
pub(crate) async fn read_message<R: AsyncRead + Unpin, T: DeserializeOwned>(
    reader: &mut R,
    max: usize,
) -> Result<T> {
    let len = reader.read_u32().await.map_err(|_| Error::Transport)? as usize;
    if len == 0 || len > max {
        return Err(Error::Protocol);
    }
    let mut data = vec![0; len];
    reader
        .read_exact(&mut data)
        .await
        .map_err(|_| Error::Transport)?;
    serde_json::from_slice(&data).map_err(|_| Error::Protocol)
}

/// One opaque application UDP datagram, scoped to its authenticated QUIC connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatagramFrame {
    pub service_handle: u32,
    pub association_id: u64,
    pub generation: u32,
    pub sequence: u32,
    pub payload: Bytes,
}
impl DatagramFrame {
    pub fn encode(&self) -> Result<Bytes> {
        if self.payload.len() > 65507 {
            return Err(Error::DatagramTooLarge);
        }
        let mut data = Vec::with_capacity(DATAGRAM_HEADER_LEN + self.payload.len());
        data.extend_from_slice(&[SCHEMA_VERSION, 0]);
        data.extend_from_slice(&self.service_handle.to_be_bytes());
        data.extend_from_slice(&self.association_id.to_be_bytes());
        data.extend_from_slice(&self.generation.to_be_bytes());
        data.extend_from_slice(&self.sequence.to_be_bytes());
        data.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        data.extend_from_slice(&self.payload);
        Ok(data.into())
    }
    pub fn decode(data: Bytes) -> Result<Self> {
        if data.len() < DATAGRAM_HEADER_LEN || data[0] != SCHEMA_VERSION || data[1] != 0 {
            return Err(Error::Protocol);
        }
        let len = u16::from_be_bytes([data[22], data[23]]) as usize;
        if data.len() != DATAGRAM_HEADER_LEN + len {
            return Err(Error::Protocol);
        }
        Ok(Self {
            service_handle: u32::from_be_bytes(data[2..6].try_into().map_err(|_| Error::Protocol)?),
            association_id: u64::from_be_bytes(
                data[6..14].try_into().map_err(|_| Error::Protocol)?,
            ),
            generation: u32::from_be_bytes(data[14..18].try_into().map_err(|_| Error::Protocol)?),
            sequence: u32::from_be_bytes(data[18..22].try_into().map_err(|_| Error::Protocol)?),
            payload: data.slice(DATAGRAM_HEADER_LEN..),
        })
    }
}

/// Bounded duplicate window: accept reordering, reject duplicate/obsolete envelopes.
#[derive(Default)]
pub(crate) struct SequenceWindow {
    high: Option<u32>,
    seen: u64,
}
impl SequenceWindow {
    pub fn accept(&mut self, sequence: u32) -> bool {
        match self.high {
            None => {
                self.high = Some(sequence);
                self.seen = 1;
                true
            }
            Some(high) if sequence > high => {
                let shift = sequence - high;
                self.seen = if shift >= 64 {
                    1
                } else {
                    (self.seen << shift) | 1
                };
                self.high = Some(sequence);
                true
            }
            Some(high) => {
                let offset = high - sequence;
                if offset >= 64 || self.seen & (1u64 << offset) != 0 {
                    return false;
                }
                self.seen |= 1u64 << offset;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_roundtrip_and_strict_length() {
        for payload in [
            Bytes::new(),
            Bytes::from_static(b"\0abc\xff"),
            Bytes::from(vec![7; 65507]),
        ] {
            let f = DatagramFrame {
                service_handle: 7,
                association_id: 9,
                generation: 3,
                sequence: 8,
                payload,
            };
            let encoded = f.encode().unwrap();
            assert_eq!(DatagramFrame::decode(encoded.clone()).unwrap(), f);
            assert!(DatagramFrame::decode(encoded.slice(..encoded.len() - 1)).is_err());
        }
        for size in 0..24 {
            assert!(DatagramFrame::decode(Bytes::from(vec![0; size])).is_err());
        }
    }
    #[test]
    fn sequence_reorders_and_drops_duplicates() {
        let mut w = SequenceWindow::default();
        assert!(w.accept(3));
        assert!(w.accept(1));
        assert!(w.accept(2));
        assert!(!w.accept(2));
        assert!(w.accept(100));
        assert!(!w.accept(3));
        assert!(w.accept(99));
        assert!(!w.accept(99));
    }
}
