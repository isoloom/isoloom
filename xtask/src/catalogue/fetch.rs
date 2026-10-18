//! Fetching over HTTPS for the refresh, through `curl` (on every host: macOS, Linux, Windows 10
//! and later ship it), so the xtask carries no TLS stack. Only the refresh fetches anything:
//! tests and generation stay offline.

use std::process::Command;

use crate::{Res, process};

/// The body at `url`, following redirects; `headers` sent with the request.
pub(crate) fn get(url: &str, headers: &[String]) -> Res<Vec<u8>> {
    let mut cmd = curl(headers);
    cmd.arg(url);
    let out = cmd.output().map_err(|e| format!("`curl` couldn't start: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("GET {url} failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into())
    }
}

/// The body at `url`, as text.
pub(crate) fn text(url: &str, headers: &[String]) -> Res<String> {
    String::from_utf8(get(url, headers)?).map_err(|_| format!("{url} isn't text").into())
}

/// The headers and the body at `url` (the last response's, after redirects).
pub(crate) fn with_headers(url: &str, headers: &[String]) -> Res<(String, Vec<u8>)> {
    let mut cmd = curl(headers);
    cmd.args(["--include", url]);
    let out = cmd.output().map_err(|e| format!("`curl` couldn't start: {e}"))?;
    if !out.status.success() {
        return Err(format!("GET {url} failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    let at = find(&out.stdout, b"\r\n\r\n").ok_or_else(|| format!("{url}: no headers in the answer"))?;
    let (head, body) = out.stdout.split_at(at);
    Ok((String::from_utf8_lossy(head).into_owned(), body.get(4..).unwrap_or_default().to_vec()))
}

/// `curl`, failing on an HTTP error, quiet but for errors, with a time limit and `headers`.
fn curl(headers: &[String]) -> Command {
    let mut cmd = Command::new("curl");
    process::without_run_env(&mut cmd);
    cmd.args(["--fail", "--silent", "--show-error", "--location", "--max-time", "180", "--proto", "=https"]);
    for h in headers {
        cmd.args(["--header", h]);
    }
    cmd
}

/// Where `needle` first is in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The value of header `name` (any case) in `head`.
pub(crate) fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_is_found_in_any_case() {
        let head = "HTTP/1.1 200 OK\r\nDocker-Content-Digest: sha256:ab\r\nContent-Type: x\r\n";
        assert_eq!(header(head, "docker-content-digest"), Some("sha256:ab"));
        assert_eq!(header(head, "etag"), None);
    }
}
