#[cfg(feature = "loopback")]
use crate::dispatch::dispatch_frame;
#[cfg(not(feature = "loopback"))]
use crate::dispatch::dispatch_frame_at_us;
use crate::dispatch::{DispatchOutcome, DispatchState, IdentifyInfo};
use crate::led::{LedInputs, WATCHDOG_WINDOW_MS};
use crate::protocol::{DeviceCommand, Frame, NakReason};
use crate::reports::{BootKeyboardReport, BootMouseReport, GamepadReport};
use crate::safety::{Watchdog, WatchdogPoll};

const CRC_LED_WINDOW_MS: u32 = 1000;
const CRC_ERROR_RING_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeState {
    dispatch: DispatchState,
    watchdog: Watchdog,
    last_valid_command_ms: Option<u32>,
    last_watchdog_fire_ms: Option<u32>,
    crc_error_ms: [u32; CRC_ERROR_RING_LEN],
    crc_error_count: usize,
    crc_error_next: usize,
}

impl RuntimeState {
    pub const fn new() -> Self {
        Self {
            dispatch: DispatchState::new(),
            watchdog: Watchdog::new(),
            last_valid_command_ms: None,
            last_watchdog_fire_ms: None,
            crc_error_ms: [0; CRC_ERROR_RING_LEN],
            crc_error_count: 0,
            crc_error_next: 0,
        }
    }

    pub fn dispatch_frame_at(
        &mut self,
        now_ms: u32,
        frame: Frame<'_>,
        identify: IdentifyInfo,
    ) -> DispatchOutcome {
        self.dispatch_frame_at_us(now_ms, now_ms.wrapping_mul(1000), frame, identify)
    }

    pub fn dispatch_frame_at_us(
        &mut self,
        now_ms: u32,
        now_us: u32,
        frame: Frame<'_>,
        identify: IdentifyInfo,
    ) -> DispatchOutcome {
        self.dispatch.telemetry.uptime_ms = now_ms;
        #[cfg(feature = "loopback")]
        let _ = now_us;
        #[cfg(feature = "loopback")]
        let outcome = dispatch_frame(&mut self.dispatch, frame, identify);
        #[cfg(not(feature = "loopback"))]
        let outcome = dispatch_frame_at_us(&mut self.dispatch, frame, identify, now_us);
        if outcome.command != DeviceCommand::Nak {
            self.watchdog
                .record_valid_command(now_ms, self.dispatch.watchdog_timeout_ms);
            self.last_valid_command_ms = Some(now_ms);
        }
        outcome
    }

    pub fn record_parser_nak(&mut self, now_ms: u32, reason: NakReason) {
        self.dispatch.telemetry.uptime_ms = now_ms;
        if matches!(reason, NakReason::CrcInvalid) {
            self.dispatch.telemetry.record_crc_error();
            self.record_crc_error_time(now_ms);
        } else {
            self.dispatch.telemetry.record_link_error();
        }
    }

    pub fn record_frame_dropped(&mut self, now_ms: u32) {
        self.dispatch.telemetry.uptime_ms = now_ms;
        self.dispatch.telemetry.record_frame_dropped();
    }

    pub fn poll_watchdog(&mut self, now_ms: u32) -> WatchdogPoll {
        self.dispatch.telemetry.uptime_ms = now_ms;
        let poll = self.watchdog.poll(now_ms, &mut self.dispatch);
        if poll == WatchdogPoll::Fired {
            self.last_watchdog_fire_ms = Some(now_ms);
        }
        poll
    }

    pub fn mouse_report_for_hid(&mut self) -> BootMouseReport {
        let report = self.dispatch.mouse;
        self.dispatch.mouse.x = 0;
        self.dispatch.mouse.y = 0;
        self.dispatch.mouse.wheel = 0;
        report
    }

    pub const fn keyboard_report_for_hid(&self) -> BootKeyboardReport {
        self.dispatch.keyboard
    }

    pub const fn gamepad_report_for_hid(&self) -> GamepadReport {
        self.dispatch.gamepad
    }

    pub fn led_inputs(&self, now_ms: u32) -> LedInputs {
        LedInputs {
            now_ms,
            ms_since_last_command: self
                .last_valid_command_ms
                .map(|last| now_ms.wrapping_sub(last)),
            ms_since_watchdog_fire: self
                .last_watchdog_fire_ms
                .map(|last| now_ms.wrapping_sub(last))
                .filter(|elapsed| *elapsed <= WATCHDOG_WINDOW_MS),
            crc_errors_last_second: self.crc_errors_last_second(now_ms),
        }
    }

    pub const fn dispatch_state(&self) -> DispatchState {
        self.dispatch
    }

    fn record_crc_error_time(&mut self, now_ms: u32) {
        self.crc_error_ms[self.crc_error_next] = now_ms;
        self.crc_error_next = (self.crc_error_next + 1) % CRC_ERROR_RING_LEN;
        if self.crc_error_count < CRC_ERROR_RING_LEN {
            self.crc_error_count += 1;
        }
    }

    fn crc_errors_last_second(&self, now_ms: u32) -> u32 {
        let mut count = 0;
        let mut index = 0;
        while index < self.crc_error_count {
            if now_ms.wrapping_sub(self.crc_error_ms[index]) <= CRC_LED_WINDOW_MS {
                count += 1;
            }
            index += 1;
        }
        count
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
}
