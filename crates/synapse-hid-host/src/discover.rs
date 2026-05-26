use serialport::{SerialPortInfo, SerialPortType};
use synapse_core::{SYNAPSE_PICO_HID_USB_PID, SYNAPSE_PICO_HID_USB_VID};
use tracing::{debug, warn};

use crate::error::{HidError, HidResult};
use crate::transport::HidGateway;

/// Opens the first Synapse Pico HID serial port that completes `IDENTIFY`.
///
/// # Errors
///
/// Returns [`HidError::PortNotFound`] when serial enumeration has no matching
/// Synapse Pico VID/PID candidate, or the last connection/handshake error when
/// all candidates are present but fail to open or identify.
pub fn connect_auto() -> HidResult<HidGateway> {
    let ports = available_ports()?;
    let candidates = candidate_port_names(&ports);

    if candidates.is_empty() {
        return Err(auto_port_not_found("no matching VID/PID serial ports"));
    }

    if candidates.len() > 1 {
        warn!(
            candidate_count = candidates.len(),
            ?candidates,
            "multiple Synapse Pico HID serial ports found; using first successful IDENTIFY"
        );
    }

    let mut last_error = None;
    for port_name in candidates {
        match HidGateway::connect(port_name.clone()) {
            Ok(gateway) => return Ok(gateway),
            Err(error) => {
                debug!(%port_name, ?error, "Synapse Pico HID candidate failed IDENTIFY/open");
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| auto_port_not_found("no candidate completed IDENTIFY")))
}

/// Returns serial-port names whose USB VID/PID matches the Synapse Pico HID.
#[must_use]
pub fn candidate_port_names(ports: &[SerialPortInfo]) -> Vec<String> {
    ports
        .iter()
        .filter(|port| is_synapse_pico_port(port))
        .map(|port| port.port_name.clone())
        .collect()
}

/// Returns true when the port is a USB serial port with the Synapse Pico VID/PID.
#[must_use]
pub const fn is_synapse_pico_port(port: &SerialPortInfo) -> bool {
    match &port.port_type {
        SerialPortType::UsbPort(usb) => {
            usb.vid == SYNAPSE_PICO_HID_USB_VID && usb.pid == SYNAPSE_PICO_HID_USB_PID
        }
        SerialPortType::PciPort | SerialPortType::BluetoothPort | SerialPortType::Unknown => false,
    }
}

fn available_ports() -> HidResult<Vec<SerialPortInfo>> {
    serialport::available_ports().map_err(|error| {
        auto_port_not_found(format!("serial enumeration failed: {}", error.description))
    })
}

fn auto_port_not_found(detail: impl AsRef<str>) -> HidError {
    HidError::PortNotFound {
        port_name: format!(
            "auto VID_0x{SYNAPSE_PICO_HID_USB_VID:04X} PID_0x{SYNAPSE_PICO_HID_USB_PID:04X}: {}",
            detail.as_ref()
        ),
    }
}
