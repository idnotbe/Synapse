use std::io::{self, ErrorKind, Read, Write};

use synapse_core::error_codes;
use synapse_hid_host::{
    DEVICE_COMMAND_ACK, HOST_COMMAND_MOUSE_MOVE_REL, HOST_MAGIC, HidError, HidPipeline,
    MAX_OUTSTANDING_FRAMES, PipelineResponse, encode_device_frame,
};

#[test]
fn try_send_command_reports_action_queue_full_at_window_boundary() {
    let mut transport = ScriptedTransport::new(Vec::new());
    let mut pipeline = HidPipeline::new();
    assert_eq!(pipeline.pending_inflight_len(), 0);
    assert_eq!(pipeline.window_capacity(), MAX_OUTSTANDING_FRAMES);

    for expected_seq in 1..=MAX_OUTSTANDING_FRAMES {
        let seq =
            match pipeline.try_send_command(&mut transport, HOST_COMMAND_MOUSE_MOVE_REL, payload())
            {
                Ok(seq) => seq,
                Err(error) => panic!("{expected_seq} should fit in the HID window: {error}"),
            };
        assert_eq!(seq, usize_to_u32(expected_seq));
    }

    assert_eq!(pipeline.pending_inflight_len(), MAX_OUTSTANDING_FRAMES);
    assert_eq!(transport.written.len(), MAX_OUTSTANDING_FRAMES);
    let error =
        match pipeline.try_send_command(&mut transport, HOST_COMMAND_MOUSE_MOVE_REL, payload()) {
            Ok(seq) => panic!("17th in-flight command should return queue full, got seq {seq}"),
            Err(error) => error,
        };

    assert_eq!(
        error,
        HidError::QueueFull {
            outstanding: MAX_OUTSTANDING_FRAMES,
            capacity: MAX_OUTSTANDING_FRAMES,
        }
    );
    assert_eq!(error.code(), error_codes::ACTION_QUEUE_FULL);
    assert_eq!(pipeline.pending_inflight_len(), MAX_OUTSTANDING_FRAMES);
    assert_eq!(pipeline.next_sequence(), 17);
    assert_eq!(transport.written.len(), MAX_OUTSTANDING_FRAMES);
}

#[test]
fn poll_response_drains_full_window_then_next_send_succeeds() {
    let mut responses = Vec::new();
    for seq in 1..=usize_to_u32(MAX_OUTSTANDING_FRAMES) {
        responses.extend_from_slice(&ack(seq));
    }
    let mut transport = ScriptedTransport::new(responses);
    let mut pipeline = HidPipeline::new();
    assert_eq!(pipeline.pending_inflight_len(), 0);
    assert_eq!(pipeline.next_sequence(), 1);

    for _ in 0..MAX_OUTSTANDING_FRAMES {
        pipeline
            .try_send_command(&mut transport, HOST_COMMAND_MOUSE_MOVE_REL, payload())
            .unwrap_or_else(|error| panic!("window fill should pass: {error}"));
    }
    assert_eq!(pipeline.pending_inflight_len(), MAX_OUTSTANDING_FRAMES);

    for expected_seq in 1..=usize_to_u32(MAX_OUTSTANDING_FRAMES) {
        let response = match pipeline.poll_response(&mut transport) {
            Ok(Some(response)) => response,
            Ok(None) => panic!("scripted ACK {expected_seq} should be available"),
            Err(error) => panic!("scripted ACK {expected_seq} should parse: {error}"),
        };
        assert_eq!(response, PipelineResponse::Ack { seq: expected_seq });
        assert_eq!(
            pipeline.pending_inflight_len(),
            MAX_OUTSTANDING_FRAMES - expected_seq as usize
        );
    }

    assert_eq!(pipeline.pending_inflight_len(), 0);
    let seq =
        match pipeline.try_send_command(&mut transport, HOST_COMMAND_MOUSE_MOVE_REL, payload()) {
            Ok(seq) => seq,
            Err(error) => panic!("drained window should accept the next command: {error}"),
        };
    assert_eq!(seq, 17);
    assert_eq!(pipeline.pending_inflight_len(), 1);
    assert_eq!(pipeline.next_sequence(), 18);
    assert_eq!(transport.written.len(), MAX_OUTSTANDING_FRAMES + 1);
    assert_eq!(
        host_frame_seq(&transport.written[MAX_OUTSTANDING_FRAMES]),
        17
    );
}

#[test]
fn zero_configured_window_clamps_to_one_and_fails_closed() {
    let mut transport = ScriptedTransport::new(Vec::new());
    let mut pipeline = HidPipeline::with_config(synapse_hid_host::PipelineConfig {
        max_outstanding: 0,
        ack_timeout_ms: 0,
        max_retries: 0,
        retry_backoff_ms: [0, 0, 0],
    });

    assert_eq!(pipeline.window_capacity(), 1);
    let first =
        match pipeline.try_send_command(&mut transport, HOST_COMMAND_MOUSE_MOVE_REL, payload()) {
            Ok(seq) => seq,
            Err(error) => panic!("clamped one-frame window should accept first command: {error}"),
        };
    assert_eq!(first, 1);
    let error =
        match pipeline.try_send_command(&mut transport, HOST_COMMAND_MOUSE_MOVE_REL, payload()) {
            Ok(seq) => panic!("second command should hit clamped queue full, got seq {seq}"),
            Err(error) => error,
        };

    assert_eq!(
        error,
        HidError::QueueFull {
            outstanding: 1,
            capacity: 1,
        }
    );
    assert_eq!(error.code(), error_codes::ACTION_QUEUE_FULL);
    assert_eq!(pipeline.pending_inflight_len(), 1);
    assert_eq!(pipeline.next_sequence(), 2);
    assert_eq!(transport.written.len(), 1);
}

const fn payload() -> &'static [u8] {
    &[1, 0, 2, 0]
}

fn usize_to_u32(value: usize) -> u32 {
    match u32::try_from(value) {
        Ok(value) => value,
        Err(error) => panic!("test value must fit u32: {error}"),
    }
}

fn ack(seq: u32) -> Vec<u8> {
    let payload = seq.to_le_bytes();
    let mut frame = [0u8; synapse_hid_host::MAX_FRAME_LEN];
    let len = match encode_device_frame(seq, DEVICE_COMMAND_ACK, &payload, &mut frame) {
        Ok(len) => len,
        Err(error) => panic!("ACK frame should encode: {error:?}"),
    };
    frame[..len].to_vec()
}

fn host_frame_seq(frame: &[u8]) -> u32 {
    assert_eq!(frame[0], HOST_MAGIC);
    u32::from_le_bytes([frame[3], frame[4], frame[5], frame[6]])
}

struct ScriptedTransport {
    read_data: Vec<u8>,
    read_offset: usize,
    written: Vec<Vec<u8>>,
}

impl ScriptedTransport {
    const fn new(read_data: Vec<u8>) -> Self {
        Self {
            read_data,
            read_offset: 0,
            written: Vec::new(),
        }
    }
}

impl Read for ScriptedTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.read_offset >= self.read_data.len() {
            return Err(io::Error::new(ErrorKind::TimedOut, "scripted timeout"));
        }

        let remaining = self.read_data.len() - self.read_offset;
        let count = remaining.min(buffer.len());
        buffer[..count]
            .copy_from_slice(&self.read_data[self.read_offset..self.read_offset + count]);
        self.read_offset += count;
        Ok(count)
    }
}

impl Write for ScriptedTransport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.written.push(buffer.to_vec());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
