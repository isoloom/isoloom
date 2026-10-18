//! Numbers derived from names that stay the same on every build and platform (unlike std's
//! hasher): the QEMU links' ports and the MAC addresses of the Hyper-V and Docker hosts'
//! adapters.

/// FNV-1a of `text`.
pub(crate) fn of(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_is_fnv_1a() {
        assert_eq!(super::of(""), 0x811c_9dc5);
        assert_eq!(super::of("a"), 0xe40c_292c);
    }
}
