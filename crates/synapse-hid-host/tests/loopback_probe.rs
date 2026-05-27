use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};

use pico_hid::protocol::{ParseResult, parse_host_frame_any_command};
use synapse_hid_host::{
    DEVICE_COMMAND_ACK, DEVICE_COMMAND_PONG, HOST_COMMAND_KEY_DOWN, HOST_COMMAND_MOUSE_MOVE_REL,
    HOST_COMMAND_RELEASE_ALL, HidError, HostCommandRequest, MAX_FRAME_LEN, MAX_PAYLOAD_LEN,
    NAK_REASON_UNKNOWN_COMMAND, encode_device_frame, perform_loopback_probe,
};

#[test]
fn loopback_probe_reads_1000_pongs_in_order() {
    let payloads = (0u32..1000)
        .map(|index| {
            let bytes = index.to_le_bytes();
            vec![bytes[0], bytes[1], 0xA5]
        })
        .collect::<Vec<_>>();
    let commands = payloads
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let command = if index % 2 == 0 {
                HOST_COMMAND_MOUSE_MOVE_REL
            } else {
                HOST_COMMAND_KEY_DOWN
            };
            HostCommandRequest::new(command, payload)
        })
        .collect::<Vec<_>>();
    let mut transport = PongLoopbackTransport::new();

    let pongs = perform_loopback_probe(&mut transport, &commands)
        .unwrap_or_else(|error| panic!("1000 loopback PONGs should pass: {error}"));

    assert_eq!(pongs.len(), 1000);
    assert_eq!(transport.host_frames_processed(), 1000);
    assert!(transport.host_bytes_written() > 0);
    assert!(transport.device_bytes_read() > 0);
    let mut expected_seq = 1u32;
    for (pong, payload) in pongs.iter().zip(payloads.iter()) {
        assert_eq!(pong.seq, expected_seq);
        assert_eq!(pong.payload, *payload);
        expected_seq = expected_seq.wrapping_add(1);
    }
    assert_eq!(transport.pending_rx_len(), 0);
    assert_eq!(transport.pending_tx_len(), 0);
}

#[test]
fn loopback_probe_accepts_empty_command_list() {
    let mut transport = PongLoopbackTransport::new();
    let pongs = perform_loopback_probe(&mut transport, &[])
        .unwrap_or_else(|error| panic!("empty loopback probe should pass: {error}"));

    assert!(pongs.is_empty());
    assert_eq!(transport.host_frames_processed(), 0);
    assert_eq!(transport.host_bytes_written(), 0);
    assert_eq!(transport.device_bytes_read(), 0);
}

#[test]
fn loopback_probe_accepts_empty_and_max_payloads_with_unknown_command() {
    let payloads = [Vec::new(), vec![0xA5; MAX_PAYLOAD_LEN]];
    let commands = [
        HostCommandRequest::new(HOST_COMMAND_RELEASE_ALL, payloads[0].as_slice()),
        HostCommandRequest::new(0xFE, payloads[1].as_slice()),
    ];
    let mut transport = PongLoopbackTransport::new();

    let pongs = perform_loopback_probe(&mut transport, &commands)
        .unwrap_or_else(|error| panic!("empty and max PONG payloads should pass: {error}"));

    assert_eq!(pongs.len(), 2);
    assert_eq!(pongs[0].seq, 1);
    assert_eq!(pongs[0].payload, payloads[0]);
    assert_eq!(pongs[1].seq, 2);
    assert_eq!(pongs[1].payload, payloads[1]);
    assert_eq!(transport.host_frames_processed(), 2);
}

#[test]
fn loopback_probe_rejects_non_pong_response() {
    let payload = [0x04];
    let command = HostCommandRequest::new(HOST_COMMAND_KEY_DOWN, &payload);
    let mut transport = PongLoopbackTransport::with_response_command(DEVICE_COMMAND_ACK);

    let error = match perform_loopback_probe(&mut transport, &[command]) {
        Ok(pongs) => panic!("non-PONG response should fail, got {pongs:?}"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        HidError::CommandRejected {
            seq: 1,
            command: DEVICE_COMMAND_ACK,
            reason: NAK_REASON_UNKNOWN_COMMAND,
        }
    );
}

#[test]
fn loopback_probe_rejects_wrong_sequence_pong() {
    let payload = [0x04];
    let command = HostCommandRequest::new(HOST_COMMAND_KEY_DOWN, &payload);
    let mut transport = PongLoopbackTransport::with_sequence_offset(10);

    let error = match perform_loopback_probe(&mut transport, &[command]) {
        Ok(pongs) => panic!("wrong-sequence PONG should fail, got {pongs:?}"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        HidError::CommandRejected {
            seq: 11,
            command: DEVICE_COMMAND_PONG,
            reason: synapse_hid_host::NAK_REASON_PAYLOAD_INVALID,
        }
    );
}

#[derive(Debug)]
struct PongLoopbackTransport {
    response_command: u8,
    sequence_offset: u32,
    rx: Vec<u8>,
    tx: VecDeque<u8>,
    host_bytes_written: usize,
    device_bytes_read: usize,
    host_frames_processed: usize,
}

impl PongLoopbackTransport {
    const fn new() -> Self {
        Self::with_response_command(DEVICE_COMMAND_PONG)
    }

    const fn with_response_command(response_command: u8) -> Self {
        Self {
            response_command,
            sequence_offset: 0,
            rx: Vec::new(),
            tx: VecDeque::new(),
            host_bytes_written: 0,
            device_bytes_read: 0,
            host_frames_processed: 0,
        }
    }

    const fn with_sequence_offset(sequence_offset: u32) -> Self {
        Self {
            response_command: DEVICE_COMMAND_PONG,
            sequence_offset,
            rx: Vec::new(),
            tx: VecDeque::new(),
            host_bytes_written: 0,
            device_bytes_read: 0,
            host_frames_processed: 0,
        }
    }

    const fn pending_rx_len(&self) -> usize {
        self.rx.len()
    }

    fn pending_tx_len(&self) -> usize {
        self.tx.len()
    }

    const fn host_bytes_written(&self) -> usize {
        self.host_bytes_written
    }

    const fn device_bytes_read(&self) -> usize {
        self.device_bytes_read
    }

    const fn host_frames_processed(&self) -> usize {
        self.host_frames_processed
    }

    fn process_rx(&mut self) -> io::Result<()> {
        loop {
            let consumed = match parse_host_frame_any_command(&self.rx) {
                ParseResult::Frame { frame, consumed } => {
                    let mut response = [0u8; MAX_FRAME_LEN];
                    let response_seq = frame.seq.wrapping_add(self.sequence_offset);
                    let len = encode_device_frame(
                        response_seq,
                        self.response_command,
                        frame.payload,
                        &mut response,
                    )
                    .map_err(encode_error)?;
                    self.tx.extend(response[..len].iter().copied());
                    self.host_frames_processed += 1;
                    consumed
                }
                ParseResult::NeedMore { .. } => break,
                ParseResult::Drop { consumed, .. } | ParseResult::Nak { consumed, .. } => consumed,
            };

            if consumed >= self.rx.len() {
                self.rx.clear();
            } else {
                self.rx.drain(..consumed);
            }
        }

        Ok(())
    }
}

impl Default for PongLoopbackTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Read for PongLoopbackTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.tx.is_empty() {
            return Err(io::Error::new(
                ErrorKind::TimedOut,
                "loopback transport has no queued response",
            ));
        }

        let count = buffer.len().min(self.tx.len());
        for slot in &mut buffer[..count] {
            let Some(byte) = self.tx.pop_front() else {
                return Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "loopback transport TX queue drained early",
                ));
            };
            *slot = byte;
        }
        self.device_bytes_read += count;
        Ok(count)
    }
}

impl Write for PongLoopbackTransport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.host_bytes_written += buffer.len();
        self.rx.extend_from_slice(buffer);
        self.process_rx()?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode_error(error: synapse_hid_host::protocol::EncodeError) -> io::Error {
    io::Error::new(
        ErrorKind::InvalidData,
        format!("loopback transport failed to encode response: {error:?}"),
    )
}
