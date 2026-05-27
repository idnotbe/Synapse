use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use crate::error::{HidError, HidResult};
use crate::pipeline::{
    HostCommandRequest, NAK_REASON_LEN_INVALID, NAK_REASON_PAYLOAD_INVALID,
    NAK_REASON_UNKNOWN_COMMAND,
};
use crate::protocol::{
    DEVICE_COMMAND_PONG, EncodeError, MAX_FRAME_LEN, ParseError, encode_host_frame,
    parse_device_frame_prefix,
};

pub const LOOPBACK_FIRST_SEQUENCE: u32 = 1;
pub const LOOPBACK_RESPONSE_TIMEOUT_MS: u64 = 200;

const READ_CHUNK_LEN: usize = 64;
const MAX_RX_BUFFER_LEN: usize = MAX_FRAME_LEN * 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoopbackProbeConfig {
    pub first_sequence: u32,
    pub response_timeout_ms: u64,
}

impl LoopbackProbeConfig {
    #[must_use]
    pub const fn m4_default() -> Self {
        Self {
            first_sequence: LOOPBACK_FIRST_SEQUENCE,
            response_timeout_ms: LOOPBACK_RESPONSE_TIMEOUT_MS,
        }
    }
}

impl Default for LoopbackProbeConfig {
    fn default() -> Self {
        Self::m4_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopbackPong {
    pub seq: u32,
    pub payload: Vec<u8>,
}

/// Sends host command frames to loopback firmware and reads matching `PONG`s.
///
/// # Errors
///
/// Returns a HID link/protocol error when a frame cannot be written, when a
/// matching `PONG` does not arrive before the configured timeout, or when the
/// firmware returns a non-`PONG`, wrong sequence, or mismatched payload.
pub fn perform_loopback_probe<T>(
    transport: &mut T,
    commands: &[HostCommandRequest<'_>],
) -> HidResult<Vec<LoopbackPong>>
where
    T: Read + Write + ?Sized,
{
    perform_loopback_probe_with_config(transport, commands, LoopbackProbeConfig::default())
}

/// Sends host command frames to loopback firmware with explicit probe config.
///
/// # Errors
///
/// Returns a HID link/protocol error when a frame cannot be written, when a
/// matching `PONG` does not arrive before the configured timeout, or when the
/// firmware returns a non-`PONG`, wrong sequence, or mismatched payload.
pub fn perform_loopback_probe_with_config<T>(
    transport: &mut T,
    commands: &[HostCommandRequest<'_>],
    config: LoopbackProbeConfig,
) -> HidResult<Vec<LoopbackPong>>
where
    T: Read + Write + ?Sized,
{
    let mut rx = Vec::new();
    let mut seq = config.first_sequence;
    let mut pongs = Vec::with_capacity(commands.len());

    for request in commands {
        write_loopback_command(transport, seq, *request, config.response_timeout_ms)?;
        let frame = read_loopback_frame(transport, &mut rx, config.response_timeout_ms)?;
        validate_pong_frame(
            seq,
            frame.seq,
            request.payload,
            frame.command,
            &frame.payload,
        )?;
        pongs.push(LoopbackPong {
            seq: frame.seq,
            payload: frame.payload,
        });
        seq = seq.wrapping_add(1);
    }

    Ok(pongs)
}

fn write_loopback_command<T>(
    transport: &mut T,
    seq: u32,
    request: HostCommandRequest<'_>,
    timeout_ms: u64,
) -> HidResult<()>
where
    T: Write + ?Sized,
{
    let mut frame = vec![0u8; MAX_FRAME_LEN];
    let len = encode_host_frame(seq, request.command, request.payload, &mut frame)
        .map_err(|error| encode_error(seq, request.command, error))?;
    transport
        .write_all(&frame[..len])
        .map_err(|_error| link_timeout("writing loopback command", timeout_ms))?;
    transport
        .flush()
        .map_err(|_error| link_timeout("flushing loopback command", timeout_ms))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeviceFrameOwned {
    seq: u32,
    command: u8,
    payload: Vec<u8>,
}

fn read_loopback_frame<T>(
    transport: &mut T,
    rx: &mut Vec<u8>,
    timeout_ms: u64,
) -> HidResult<DeviceFrameOwned>
where
    T: Read + ?Sized,
{
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if let Some(frame) = try_parse_device_frame(rx, timeout_ms)? {
            return Ok(frame);
        }

        if Instant::now() >= deadline {
            return Err(link_timeout("reading loopback PONG", timeout_ms));
        }

        let mut chunk = [0u8; READ_CHUNK_LEN];
        match transport.read(&mut chunk) {
            Ok(0) => {}
            Ok(count) => {
                if rx.len() + count > MAX_RX_BUFFER_LEN {
                    rx.clear();
                    return Err(link_timeout("reading loopback PONG", timeout_ms));
                }
                rx.extend_from_slice(&chunk[..count]);
            }
            Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(_error) => return Err(link_timeout("reading loopback PONG", timeout_ms)),
        }
    }
}

fn try_parse_device_frame(
    rx: &mut Vec<u8>,
    timeout_ms: u64,
) -> HidResult<Option<DeviceFrameOwned>> {
    loop {
        match parse_device_frame_prefix(rx) {
            Ok((frame, consumed)) => {
                let owned = DeviceFrameOwned {
                    seq: frame.seq,
                    command: frame.command,
                    payload: frame.payload.to_vec(),
                };
                rx.drain(..consumed);
                return Ok(Some(owned));
            }
            Err(ParseError::NeedMore { .. }) => return Ok(None),
            Err(
                ParseError::BadMagic { .. }
                | ParseError::LenTooShort { .. }
                | ParseError::LenOverflow { .. },
            ) => {
                if rx.is_empty() {
                    return Ok(None);
                }
                rx.remove(0);
            }
            Err(ParseError::CrcInvalid { .. }) => {
                rx.clear();
                return Err(link_timeout("reading loopback PONG", timeout_ms));
            }
        }
    }
}

fn validate_pong_frame(
    expected_seq: u32,
    actual_seq: u32,
    expected_payload: &[u8],
    command: u8,
    payload: &[u8],
) -> HidResult<()> {
    if actual_seq != expected_seq {
        return Err(HidError::CommandRejected {
            seq: actual_seq,
            command,
            reason: NAK_REASON_PAYLOAD_INVALID,
        });
    }

    if command != DEVICE_COMMAND_PONG {
        return Err(HidError::CommandRejected {
            seq: expected_seq,
            command,
            reason: NAK_REASON_UNKNOWN_COMMAND,
        });
    }

    if payload != expected_payload {
        return Err(HidError::CommandRejected {
            seq: expected_seq,
            command,
            reason: NAK_REASON_PAYLOAD_INVALID,
        });
    }

    Ok(())
}

const fn encode_error(seq: u32, command: u8, error: EncodeError) -> HidError {
    let reason = match error {
        EncodeError::PayloadTooLarge => NAK_REASON_PAYLOAD_INVALID,
        EncodeError::OutputTooSmall { .. } => NAK_REASON_LEN_INVALID,
    };
    HidError::CommandRejected {
        seq,
        command,
        reason,
    }
}

const fn link_timeout(operation: &'static str, timeout_ms: u64) -> HidError {
    HidError::LinkTimeout {
        operation,
        timeout_ms,
    }
}
