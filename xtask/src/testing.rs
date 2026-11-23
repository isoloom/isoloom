//! What the xtask's tests share: an archive of one file, and the calls the stand-ins saw.

/// A tar archive holding one file, `name`, with `bytes`.
pub(crate) fn tar_of(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(bytes.len()).unwrap());
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, name, bytes).unwrap();
    tar.into_inner().unwrap()
}

/// The calls `bin`'s stand-ins logged, each as one line.
#[cfg(unix)]
pub(crate) fn said(bin: &isoloom_test_support::FakeBin) -> Vec<String> {
    bin.calls().unwrap().iter().map(|c| c.join(" ")).collect()
}
