//! Bytes as text: hexadecimal for digests, base64 for PowerShell's encoded commands.

use std::fmt::Write as _;

/// Lowercase hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Standard base64, with padding.
pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            let sextet = (n >> (18 - 6 * i)) & 63;
            out.push(if i <= chunk.len() { char::from(ABC[sextet as usize]) } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_as_the_standard_says() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Many"), "TWFueQ==");
    }

    #[test]
    fn hex_is_two_lowercase_digits_a_byte() {
        assert_eq!(hex(&[0, 0xab, 0x10]), "00ab10");
    }
}
