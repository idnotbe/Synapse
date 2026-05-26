use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
use synapse_core::{SYNAPSE_PICO_HID_USB_PID, SYNAPSE_PICO_HID_USB_VID};
use synapse_hid_host::{candidate_port_names, is_synapse_pico_port};

#[test]
fn candidate_filter_accepts_synapse_vid_pid_usb_serial_ports() {
    let ports = vec![
        usb_port("COM7", SYNAPSE_PICO_HID_USB_VID, SYNAPSE_PICO_HID_USB_PID),
        usb_port("COM8", 0x2E8A, 0x000A),
        unknown_port("COM9"),
        usb_port("COM10", SYNAPSE_PICO_HID_USB_VID, SYNAPSE_PICO_HID_USB_PID),
    ];

    assert!(is_synapse_pico_port(&ports[0]));
    assert!(!is_synapse_pico_port(&ports[1]));
    assert!(!is_synapse_pico_port(&ports[2]));
    assert_eq!(candidate_port_names(&ports), vec!["COM7", "COM10"]);
}

#[test]
fn candidate_filter_rejects_non_usb_ports_even_when_named_like_com_ports() {
    let ports = vec![
        SerialPortInfo {
            port_name: "COM1".to_string(),
            port_type: SerialPortType::PciPort,
        },
        SerialPortInfo {
            port_name: "COM2".to_string(),
            port_type: SerialPortType::BluetoothPort,
        },
    ];

    assert!(candidate_port_names(&ports).is_empty());
}

fn usb_port(port_name: &str, vid: u16, pid: u16) -> SerialPortInfo {
    SerialPortInfo {
        port_name: port_name.to_string(),
        port_type: SerialPortType::UsbPort(UsbPortInfo {
            vid,
            pid,
            serial_number: Some("SYN-PICO-HID-TEST".to_string()),
            manufacturer: Some("Synapse".to_string()),
            product: Some("Synapse Pico HID".to_string()),
        }),
    }
}

fn unknown_port(port_name: &str) -> SerialPortInfo {
    SerialPortInfo {
        port_name: port_name.to_string(),
        port_type: SerialPortType::Unknown,
    }
}
