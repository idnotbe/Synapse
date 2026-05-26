use synapse_core::Key;

use crate::ActionError;

pub(super) use super::keymap::HidKeyboardKey;

pub(super) fn hid_key(key: &Key) -> Result<HidKeyboardKey, ActionError> {
    super::keymap::hid_key(key)
}

pub(super) fn hid_text_key(ch: char) -> Result<HidKeyboardKey, ActionError> {
    super::keymap::hid_text_key(ch)
}
