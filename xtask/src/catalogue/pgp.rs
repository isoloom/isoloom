//! A GPG key file's primary keys' fingerprints (RFC 4880, version 4: the SHA-1 of the key
//! packet), read from its ASCII armour, so the refresh checks a repository key against the
//! fingerprint its vendor publishes before pinning the file's SHA-256.

use sha1::{Digest, Sha1};

use crate::text::hex;

/// Every primary key's fingerprint in `armored` (one or more armour blocks), in uppercase hex.
pub(crate) fn fingerprints(armored: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for block in armored.split("-----BEGIN PGP PUBLIC KEY BLOCK-----").skip(1) {
        let body = block.split("-----END").next().unwrap_or_default();
        // The armour's headers end at its first blank line; the checksum line starts with `=`.
        let lines: Vec<&str> = body.lines().skip(1).map(str::trim).collect();
        let start = lines.iter().position(|l| l.is_empty()).map_or(0, |i| i + 1);
        let data: String = lines.iter().skip(start).filter(|l| !l.starts_with('=')).copied().collect();
        out.extend(primary_keys(&decode(&data)?)?);
    }
    if out.is_empty() {
        return Err("no public key block in the file".to_owned());
    }
    Ok(out)
}

/// The fingerprints of the primary key packets (tag 6) in `bytes`.
fn primary_keys(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some((&head, after)) = rest.split_first() {
        let (tag, len, after) = header(head, after)?;
        let body = after.get(..len).ok_or("a packet runs past the end of the key")?;
        if tag == 6 {
            let size = u16::try_from(len).map_err(|_| "a key packet too long for a version-4 fingerprint")?;
            let mut h = Sha1::new();
            h.update([0x99]);
            h.update(size.to_be_bytes());
            h.update(body);
            out.push(hex(&h.finalize()).to_ascii_uppercase());
        }
        rest = after.get(len..).unwrap_or_default();
    }
    Ok(out)
}

/// A packet's tag and length from its first byte `head`, and what follows the header.
fn header(head: u8, after: &[u8]) -> Result<(u8, usize, &[u8]), String> {
    let short = |n: usize| -> Result<(usize, &[u8]), String> {
        let bytes = after.get(..n).ok_or("a packet header runs past the end")?;
        Ok((
            bytes.iter().fold(0usize, |acc, b| (acc << 8) | usize::from(*b)),
            after.get(n..).unwrap_or_default(),
        ))
    };
    if head & 0x80 == 0 {
        return Err("not an OpenPGP packet".to_owned());
    }
    if head & 0x40 == 0 {
        // The old format: the tag in bits 5-2, the length's size in bits 1-0.
        let (len, rest) = match head & 3 {
            0 => short(1)?,
            1 => short(2)?,
            2 => short(4)?,
            _ => return Err("a packet of indeterminate length".to_owned()),
        };
        return Ok(((head >> 2) & 0x0f, len, rest));
    }
    let tag = head & 0x3f;
    let (&first, rest) = after.split_first().ok_or("a packet header runs past the end")?;
    let (len, rest) = match first {
        0..=191 => (usize::from(first), rest),
        192..=223 => {
            let (&second, rest) = rest.split_first().ok_or("a packet header runs past the end")?;
            (((usize::from(first) - 192) << 8) + usize::from(second) + 192, rest)
        }
        255 => {
            let bytes = rest.get(..4).ok_or("a packet header runs past the end")?;
            (
                bytes.iter().fold(0usize, |acc, b| (acc << 8) | usize::from(*b)),
                rest.get(4..).unwrap_or_default(),
            )
        }
        _ => return Err("a packet of partial length".to_owned()),
    };
    Ok((tag, len, rest))
}

/// Standard base64 decoded, whitespace ignored.
fn decode(text: &str) -> Result<Vec<u8>, String> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        acc = (acc << 6) | u32::from(value(c).ok_or("not base64")?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).map_err(|_| "not base64")?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::base64;

    /// An armour block around `packets`.
    fn armored(packets: &[u8]) -> String {
        format!(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\nComment: test\n\n{}\n=abcd\n-----END PGP PUBLIC KEY BLOCK-----\n",
            base64(packets)
        )
    }

    #[test]
    fn base64_reads_back_what_it_wrote() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"\x00\xff\x10\x99"] {
            assert_eq!(decode(&base64(bytes)).unwrap(), bytes);
        }
    }

    #[test]
    fn each_primary_key_has_its_fingerprint_and_subkeys_none() {
        // A new-format key packet (tag 6) and subkey packet (tag 14), and an old-format user id.
        let key = [0xc6, 3, 4, 1, 2];
        let subkey = [0xce, 2, 4, 9];
        let uid = [0xb4, 2, b'h', b'i'];
        let all = [&key[..], &subkey, &uid].concat();
        let mut h = Sha1::new();
        h.update([0x99, 0, 3, 4, 1, 2]);
        let expected = hex(&h.finalize()).to_ascii_uppercase();
        assert_eq!(fingerprints(&armored(&all)).unwrap(), std::slice::from_ref(&expected));
        let two = format!("{}{}", armored(&key), armored(&key));
        assert_eq!(fingerprints(&two).unwrap(), [expected.clone(), expected]);
        assert!(fingerprints("no key here").is_err());
    }
}
