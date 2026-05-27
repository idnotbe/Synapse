use sha2::{Digest, Sha256};

#[must_use]
pub fn package_manifest_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{}", hex_lower(&digest))
}

pub fn same_digest(left: &str, right: &str) -> bool {
    left.strip_prefix("sha256:")
        .zip(right.strip_prefix("sha256:"))
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}
