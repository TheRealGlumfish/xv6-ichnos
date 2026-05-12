use crate::xv6::memory::MemoryInfo;

use super::memory::MemInfo;
use super::syscall::{Syscall, SyscallEvent};
use super::{process, process::Process};

use futures::{SinkExt, StreamExt};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use tokio::net::UnixStream;
use tokio_util::bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use std::{io, mem};

/// xv6-ichnos RPC protocol magic number.
pub const MAGIC: u32 = 0x67676767;

#[allow(clippy::upper_case_acronyms)] // TODO: Decide on the naming convention for these "C" types
#[derive(TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
enum RpcTag {
    MAGIC,
    HEARTBEAT,
    PSTAT,
    KILL,
    TRACE,
    GETTRACE,
    EXEC,
    MEMINFO,
}

impl From<&RpcReq> for RpcTag {
    fn from(req: &RpcReq) -> Self {
        match req {
            RpcReq::Magic => RpcTag::MAGIC,
            RpcReq::Heartbeat => RpcTag::HEARTBEAT,
            RpcReq::PStat => RpcTag::PSTAT,
            RpcReq::Kill(_) => RpcTag::KILL,
            RpcReq::Trace { .. } => RpcTag::TRACE,
            RpcReq::GetTrace(_) => RpcTag::GETTRACE,
            RpcReq::Exec(_) => RpcTag::EXEC,
            RpcReq::MemInfo(_) => RpcTag::MEMINFO,
        }
    }
}

/// An xv6-ichnos RPC request.
#[derive(Clone)]
pub enum RpcReq {
    Magic,
    Heartbeat,
    PStat,
    Kill(i32),
    Trace { mask: u32, pid: i32 },
    GetTrace(i32),
    Exec(String),
    MemInfo(i32),
}

/// A conversion of an `RpcReq` to bytes for sending over the RPC channel.
impl From<RpcReq> for Bytes {
    fn from(req: RpcReq) -> Self {
        match req {
            RpcReq::Magic | RpcReq::Heartbeat | RpcReq::PStat => {
                Bytes::copy_from_slice(&[RpcTag::from(&req).into()])
            }
            RpcReq::Kill(pid) => {
                let mut payload = BytesMut::with_capacity(5);
                payload.put_u8(RpcTag::from(&req).into());
                payload.put_i32_le(pid);
                payload.freeze()
            }
            RpcReq::Trace { mask, pid } => {
                println!("[RpcReq::Trace] mask: {:#b}, pid: {}", mask, pid);
                let mut payload = BytesMut::with_capacity(9);
                payload.put_u8(RpcTag::from(&req).into());
                payload.put_i32_le(pid);
                payload.put_u32_le(mask);
                payload.freeze()
            }
            RpcReq::GetTrace(pid) => {
                let mut payload = BytesMut::with_capacity(5);
                payload.put_u8(RpcTag::from(&req).into());
                payload.put_i32_le(pid);
                payload.freeze()
            }
            RpcReq::Exec(ref file) => {
                let mut payload = BytesMut::with_capacity(1 + file.len() + 1);
                payload.put_u8(RpcTag::from(&req).into());
                payload.extend_from_slice(file.as_bytes());
                payload.put_u8(b'\0');
                payload.freeze()
            }
            RpcReq::MemInfo(pid) => {
                let mut payload = BytesMut::with_capacity(5);
                payload.put_u8(RpcTag::from(&req).into());
                payload.put_i32_le(pid);
                payload.freeze()
            }
        }
    }
}

impl RpcReq {
    /// Return the PID associated with this request.
    /// Panics if the request type does not have an associated PID.
    pub fn get_pid(&self) -> i32 {
        match self {
            RpcReq::Kill(pid) => *pid,
            RpcReq::Trace { pid, .. } => *pid,
            RpcReq::GetTrace(pid) => *pid,
            RpcReq::MemInfo(pid) => *pid,
            _ => panic!("This request type does not have an associated PID"),
        }
    }
}

/// An xv6-ichnos RPC response.
#[derive(Clone)]
pub enum RpcResp {
    Magic(u32),
    Heartbeat(i32),
    PStat(Vec<Process>),
    Kill(bool),
    Trace(bool),
    GetTrace(Vec<Syscall>),
    Exec(i32), // TODO: Can we ever return an error message here? Maybe we should return the PID?
    MemInfo(Option<MemoryInfo>),
}

/// A conversion of bytes received from the RPC channel to an `RpcResp`.
impl TryFrom<BytesMut> for RpcResp {
    type Error = io::Error; // TODO: Switch to custom error type

    fn try_from(frame: BytesMut) -> io::Result<RpcResp> {
        let resp_type = RpcTag::try_from(frame[0])
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?; // TODO: I think its guaranteed with have the first elem but we should probably check
        let mut payload = &frame[1..];
        match resp_type {
            RpcTag::MAGIC => {
                if payload.len() == mem::size_of::<u32>() {
                    Ok(RpcResp::Magic(payload.get_u32_le()))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Magic response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::HEARTBEAT => {
                if payload.len() == mem::size_of::<i32>() {
                    Ok(RpcResp::Heartbeat(payload.get_i32_le()))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Heartbeat response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::PSTAT => {
                if payload.len().is_multiple_of(process::UPROC_SZ) {
                    Ok(RpcResp::PStat(
                        payload
                            .chunks_exact(process::UPROC_SZ)
                            .map(Process::try_from)
                            .collect::<io::Result<Vec<Process>>>()?,
                    ))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("PStat response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::KILL => {
                if payload.len() == 1 {
                    Ok(RpcResp::Kill(payload[0] == 1))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Kill response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::TRACE => {
                if payload.len() == 1 {
                    Ok(RpcResp::Trace(payload[0] == 1))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Trace response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::GETTRACE => {
                if payload.len().is_multiple_of(64) {
                    Ok(RpcResp::GetTrace(
                        payload
                            .chunks_exact(64)
                            .map(SyscallEvent::try_from)
                            .map(|res| res.map(Syscall::from))
                            .collect::<io::Result<Vec<Syscall>>>()?,
                    ))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("GetTrace response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::EXEC => {
                if payload.len() == mem::size_of::<i32>() {
                    Ok(RpcResp::Exec(payload.get_i32_le()))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Exec response has invalid length {}", payload.len()),
                    ))
                }
            }
            RpcTag::MEMINFO => {
                if payload.is_empty() {
                    Ok(RpcResp::MemInfo(None))
                } else {
                    Ok(RpcResp::MemInfo(Some(MemoryInfo::from(MemInfo::try_from(
                        payload,
                    )?))))
                }
            }
        }
    }
}

// TODO: Update this comment once we move to a generic stream
/// An RPC handler for asynchronously sending and receiving RPC requests and responses over a UnixStream.
/// Internally, a length-delimited tokio coded is used to encode and decode messages.
pub struct RpcHandler {
    framed_stream: Framed<UnixStream, LengthDelimitedCodec>,
}

// TODO: Make UnixStream be a generic (use AsyncRead + AsyncWrite)
impl RpcHandler {
    /// Create a new `RpcHandler` from a connected `UnixStream`.
    pub fn new(stream: UnixStream) -> Self {
        let framed_stream = LengthDelimitedCodec::builder()
            .length_field_type::<u16>()
            .little_endian()
            .length_adjustment(1)
            .num_skip(2)
            .new_framed(stream);
        Self { framed_stream }
    }

    /// Asynchronously send an `RpcReq` and wait for the corresponding `RpcResp`.
    /// Returns an error if the stream is closed or if the response is invalid (or other I/O error).
    pub async fn send_request(&mut self, req: RpcReq) -> Result<RpcResp, io::Error> {
        let req_bytes = Bytes::from(req);
        self.framed_stream.send(req_bytes).await?;
        if let Some(frame) = self.framed_stream.next().await {
            let frame = frame?;
            RpcResp::try_from(frame)
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Stream closed while waiting for response",
            ))
        }
    }
}
