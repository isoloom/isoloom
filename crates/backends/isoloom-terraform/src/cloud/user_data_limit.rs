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
    /// DigitalOcean: 64 KiB of user data, plain text.
    /// <https://docs.digitalocean.com/products/droplets/how-to/provide-user-data/>
    DigitalOcean,
    /// Linode: 65535 bytes of user data, once decoded from its base64.
    /// <https://techdocs.akamai.com/linode-api/reference/post-linode-instance>
    Linode,
    /// Oracle Cloud: 32000 bytes for all of an instance's metadata, the user data counted in the
    /// base64 it travels in, less [`OCI_KEY_ROOM`] for the SSH key beside it.
    /// <https://docs.oracle.com/iaas/Content/Compute/Tasks/updatinginstancemetada.htm>
    Oci,
}

/// What Oracle Cloud's cap leaves for the instance's other metadata value, its SSH public key
/// (`ssh_authorized_keys`): an OpenSSH key, RSA 4096 included, is under 1 KiB.
const OCI_KEY_ROOM: usize = 1024;

impl Limit {
    /// The cap in bytes.
    #[must_use]
    pub(crate) fn bytes(self) -> usize {
        match self {
            Limit::Aws => 16 * 1024,
            Limit::Azure | Limit::Linode => 65_535,
            Limit::Gcp => 256 * 1024,
            Limit::DigitalOcean => 64 * 1024,
            Limit::Oci => 32_000 - OCI_KEY_ROOM,
        }
    }

    /// The size of `document` as the cloud counts it: its bytes, or (Oracle Cloud) its base64's.
    fn counted(self, document: &[u8]) -> usize {
        match self {
            Limit::Oci => document.len().div_ceil(3) * 4,
            _ => document.len(),
        }
    }

    /// The provider's name, for a refusal.
    #[must_use]
    pub(crate) fn provider(self) -> &'static str {
        match self {
            Limit::Aws => "AWS",
            Limit::Azure => "Azure",
            Limit::Gcp => "Google Cloud",
            Limit::DigitalOcean => "DigitalOcean",
            Limit::Linode => "Linode",
            Limit::Oci => "Oracle Cloud",
        }
    }

    /// `Ok` when `document` (the user data as the module sends it: gzipped when it is sent
    /// gzipped, never base64) fits, as the cloud counts it ([`Limit::counted`]); otherwise
    /// [`TooBig`], which a caller turns into a refusal naming the machine.
    ///
    /// # Errors
    ///
    /// [`TooBig`] when the document is over the cap, with its size and the cap.
    pub(crate) fn fits(self, document: &[u8]) -> Result<(), TooBig> {
        let size = self.counted(document);
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

    #[test]
    fn oracle_cloud_counts_the_base64_beside_the_ssh_key() {
        assert_eq!(Limit::Oci.bytes(), 30_976);
        // 23232 bytes are 30976 in base64: the cap's own size.
        assert_eq!(Limit::Oci.fits(&vec![b'x'; 23_232]), Ok(()));
        let err = Limit::Oci.fits(&vec![b'x'; 23_233]).unwrap_err();
        assert_eq!((err.provider, err.size, err.cap), ("Oracle Cloud", 30_980, 30_976));
        assert_eq!(Limit::DigitalOcean.fits(&vec![b'x'; 64 * 1024]), Ok(()));
        assert!(Limit::DigitalOcean.fits(&vec![b'x'; 64 * 1024 + 1]).is_err());
        assert!(Limit::Linode.fits(&vec![b'x'; 65_536]).is_err());
    }
}
