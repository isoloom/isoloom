//! Bytes as text: hexadecimal for digests, base64 for PowerShell's encoded commands.

/// Lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|b| [b >> 4, b & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// Standard base64, with padding.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().zip([16u32, 8, 0]).fold(0u32, |n, (b, shift)| n | (u32::from(*b) << shift));
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            let letter = usize::try_from((n >> shift) & 63).ok().and_then(|at| ABC.get(at));
            out.push(match letter {
                Some(l) if i <= chunk.len() => char::from(*l),
                _ => '=',
            });
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
