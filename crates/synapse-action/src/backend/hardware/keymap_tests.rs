use synapse_core::{Key, KeyCode};

use super::keymap::{hid_key, hid_text_key, hid_usage, hid_usage_for_text_char};

#[test]
fn raw_hid_codes_cover_usb_keyboard_keypad_defined_ranges() {
    let mut count = 0;
    for value in 0u8..=u8::MAX {
        let actual = hid_usage(&hid(value));
        if matches!(value, 0x04..=0xA4 | 0xB0..=0xDD | 0xE0..=0xE7) {
            assert_eq!(actual, Ok(value));
            count += 1;
        } else {
            assert!(actual.is_err(), "0x{value:02X} should be rejected");
        }
    }

    assert_eq!(count, 215);
    assert!(count > 200);
}

#[test]
fn key_codes_map_to_expected_hid_usage_samples() {
    let cases = [
        (symbol('a'), 0x04),
        (symbol('A'), 0x04),
        (symbol('!'), 0x1E),
        (symbol('\n'), 0x28),
        (symbol(' '), 0x2C),
        (symbol('?'), 0x38),
        (named("keyboard-a"), 0x04),
        (named("keyboard-0"), 0x27),
        (named("ctrl"), 0xE0),
        (named("right shift"), 0xE5),
        (named("keyboard-f24"), 0x73),
        (named("keyboard international 9"), 0x8F),
        (named("keyboard lang 9"), 0x98),
        (named("keypad 000"), 0xB1),
        (named("numpad 6"), 0x5E),
        (named("kp0"), 0x62),
        (named("keypad hexadecimal"), 0xDD),
        (named("volume_down"), 0x81),
        (hid(0x04), 0x04),
        (hid(0xA4), 0xA4),
        (hid(0xB0), 0xB0),
        (hid(0xE7), 0xE7),
    ];

    for (key, expected) in cases {
        assert_eq!(hid_usage(&key), Ok(expected));
    }
}

#[test]
fn key_codes_map_to_expected_boot_modifier_and_usage_fields() {
    let cases = [
        (symbol('a'), 0x00, Some(0x04)),
        (symbol('A'), 0x02, Some(0x04)),
        (symbol('!'), 0x02, Some(0x1E)),
        (symbol('?'), 0x02, Some(0x38)),
        (named("ctrl"), 0x01, None),
        (named("shift"), 0x02, None),
        (named("alt"), 0x04, None),
        (named("super"), 0x08, None),
        (named("right shift"), 0x20, None),
        (named("F12"), 0x00, Some(0x45)),
        (hid(0xE7), 0x80, None),
    ];

    for (key, expected_modifiers, expected_usage) in cases {
        let actual = hid_key(&key).unwrap_or_else(|error| panic!("key should map: {error}"));
        assert_eq!(actual.modifiers, expected_modifiers);
        assert_eq!(actual.key_usage, expected_usage);
    }
}

#[test]
fn text_chars_apply_left_shift_for_us_layout_shifted_characters() {
    let cases = [('A', 0x02, 0x04), ('!', 0x02, 0x1E), ('a', 0x00, 0x04)];

    for (ch, expected_modifiers, expected_usage) in cases {
        let actual = hid_text_key(ch).unwrap_or_else(|error| panic!("char should map: {error}"));
        assert_eq!(actual.modifiers, expected_modifiers);
        assert_eq!(actual.key_usage, Some(expected_usage));
        assert_eq!(hid_usage_for_text_char(ch), Ok(expected_usage));
    }
}

#[test]
fn unsupported_keys_fail_closed() {
    for key in [
        symbol('€'),
        named("synapse-not-a-key"),
        hid(0),
        hid(0xA5),
        hid(0xDE),
        hid(0xE8),
    ] {
        let error = match hid_usage(&key) {
            Ok(usage) => panic!("key should fail closed, got usage 0x{usage:02X}"),
            Err(error) => error,
        };
        assert_eq!(
            error.code(),
            synapse_core::error_codes::ACTION_UNSUPPORTED_KEY
        );
    }

    let ch = '€';
    let text_error = match hid_usage_for_text_char(ch) {
        Ok(usage) => panic!("{ch:?} should fail closed, got usage 0x{usage:02X}"),
        Err(error) => error,
    };
    assert_eq!(
        text_error.code(),
        synapse_core::error_codes::ACTION_UNSUPPORTED_KEY
    );
}

fn symbol(value: char) -> Key {
    Key {
        code: KeyCode::Symbol { value },
        use_scancode: false,
    }
}

fn named(value: &str) -> Key {
    Key {
        code: KeyCode::Named {
            value: value.to_owned(),
        },
        use_scancode: false,
    }
}

fn hid(value: u8) -> Key {
    Key {
        code: KeyCode::HidCode { value },
        use_scancode: true,
    }
}
