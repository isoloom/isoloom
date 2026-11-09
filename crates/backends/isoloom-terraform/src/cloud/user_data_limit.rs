//! How large each cloud lets a machine's user data be, and the refusal of a document over it.

/// How large a cloud lets a machine's user data be, in bytes of the document it receives once
/// decoded from the base64 it travels in (a `docker-vm`, Proxmox or ESXi snippet has no such cap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Limit {
    /// AWS: 16 KiB of user data, counted before its base64 (so, for gzipped user data, the
    /// gzip's bytes).
    /// <https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/user-data.html>
    Aws,
    /// Azure: 65535 bytes of custom data, once decoded from its base64.
    /// <https://learn.microsoft.com/rest/api/compute/virtual-machines/create-or-update#osprofile>
    Azure,
    /// Google Cloud: 256 KiB for one metadata value (the startup script).
    /// <https://cloud.google.com/compute/docs/metadata/setting-custom-metadata#limitations>
    Gcp,
}

impl Limit {
    /// The cap in bytes.
    #[must_use]
    pub(crate) fn bytes(self) -> usize {
        match self {
            Limit::Aws => 16 * 1024,
            Limit::Azure => 65_535,
            Limit::Gcp => 256 * 1024,
        }
    }

    /// The provider's name, for a refusal.
    #[must_use]
    pub(crate) fn provider(self) -> &'static str {
        match self {
            Limit::Aws => "AWS",
            Limit::Azure => "Azure",
            Limit::Gcp => "Google Cloud",
        }
    }

    /// `Ok` when `document` (the user data as the cloud counts it: gzipped when it is sent
    /// gzipped, never base64) fits; otherwise [`TooBig`], which a caller turns into a refusal
    /// naming the machine.
    ///
    /// # Errors
    ///
    /// [`TooBig`] when the document is over the cap, with its size and the cap.
    pub(crate) fn fits(self, document: &[u8]) -> Result<(), TooBig> {
        let size = document.len();
        let cap = self.bytes();
        if size <= cap {
            Ok(())
        } else {
            Err(TooBig {
                provider: self.provider(),
                size,
                cap,
            })
        }
    }
}

/// A machine's user data doesn't fit its cloud's cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("its user data is {size} bytes as {provider} counts it, over its {cap}-byte limit: fewer or smaller steps, files or recipes")]
pub(crate) struct TooBig {
    /// The provider whose cap it broke.
    pub(crate) provider: &'static str,
    /// The document's size, in bytes.
    pub(crate) size: usize,
    /// The cap, in bytes.
    pub(crate) cap: usize,
}

#[cfg(test)]
mod tests {
    use isoloom_render::configure::cloud_init::gzip;

    use super::Limit;

    #[test]
    fn a_limit_refuses_a_document_over_its_cap_naming_the_overflow() {
        let small = "x".repeat(100);
        assert_eq!(Limit::Aws.fits(small.as_bytes()), Ok(()));
        let big = "x".repeat(Limit::Aws.bytes() + 50);
        let err = Limit::Aws.fits(big.as_bytes()).unwrap_err();
        assert_eq!(err.provider, "AWS");
        assert_eq!(err.cap, 16 * 1024);
        assert_eq!(err.size, 16 * 1024 + 50);
        assert!(err.to_string().contains("16434 bytes as AWS counts it, over its 16384-byte limit"), "{err}");
        assert_eq!(Limit::Aws.fits(&gzip(&big)), Ok(()), "gzip shrinks what AWS counts");
        assert_eq!((Limit::Azure.bytes(), Limit::Gcp.bytes()), (65_535, 256 * 1024));
        assert_eq!((Limit::Azure.provider(), Limit::Gcp.provider()), ("Azure", "Google Cloud"));
    }
}
