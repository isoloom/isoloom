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
    let mut cmd = process::tool("curl");
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

/// The tests' web: a stand-in `curl` serving the pages a test gave it, each call logged.
#[cfg(test)]
#[cfg(unix)]
pub(super) mod testing {
    use isoloom_test_support::FakeBin;

    /// A stand-in `curl` serving pages: a known address answers with its body (after its
    /// headers, for `--include`), any other fails as curl does on a 404.
    pub(crate) struct Web {
        bin: FakeBin,
    }

    impl Web {
        pub(crate) fn new() -> Web {
            let bin = FakeBin::named("xtask-web").unwrap();
            let pages = bin.dir().join("pages");
            std::fs::create_dir_all(&pages).unwrap();
            let script = format!(
                r#"include=; url=
for a in "$@"; do [ "$a" = --include ] && include=1; url=$a; done
echo "curl $*" >> '{log}'
page='{pages}'/$(printf %s "$url" | tr -c 'A-Za-z0-9.-' _)
[ -f "$page" ] || {{ echo "curl: (22) The requested URL returned error: 404" >&2; exit 22; }}
if [ -n "$include" ]; then printf 'HTTP/1.1 200 OK\r\n'; cat "$page.head"; printf '\r\n'; fi
cat "$page""#,
                log = bin.log().display(),
                pages = pages.display()
            );
            bin.stand_in("curl", &script).unwrap();
            Web { bin }
        }

        /// The file holding `url`'s answer.
        fn page(&self, url: &str) -> std::path::PathBuf {
            let name: String = url
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
                .collect();
            self.bin.dir().join("pages").join(name)
        }

        /// `url` answers `body`, with `headers` (each `Name: value`).
        pub(crate) fn serves(&self, url: &str, headers: &[&str], body: &[u8]) -> &Web {
            let page = self.page(url);
            std::fs::write(&page, body).unwrap();
            std::fs::write(
                format!("{}.head", page.display()),
                headers.iter().fold(String::new(), |all, h| all + h + "\r\n"),
            )
            .unwrap();
            self
        }

        /// `f` run with this web's `curl`.
        pub(crate) fn browse<T>(&self, f: impl FnOnce() -> T) -> T {
            crate::process::testing::with_stand_ins(&self.bin.path(), f)
        }

        /// Each call, its arguments split at spaces.
        pub(crate) fn calls(&self) -> Vec<Vec<String>> {
            self.bin.calls().unwrap()
        }
    }
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

    #[cfg(unix)]
    #[test]
    fn a_body_is_fetched_over_https_only_failing_on_an_http_error_with_the_headers_given() {
        let web = testing::Web::new();
        web.serves("https://example.org/a", &[], b"body\n");
        let got = web.browse(|| get("https://example.org/a", &["Accept: x/y".to_owned()]).unwrap());
        assert_eq!(got, b"body\n");
        assert_eq!(
            web.calls(),
            [[
                "curl",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--max-time",
                "180",
                "--proto",
                "=https",
                "--header",
                "Accept:",
                "x/y",
                "https://example.org/a"
            ]]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_fetch_names_the_address_and_what_curl_said() {
        let web = testing::Web::new();
        let said = "GET https://example.org/gone failed: curl: (22) The requested URL returned error: 404";
        assert_eq!(web.browse(|| get("https://example.org/gone", &[])).unwrap_err().to_string(), said);
        assert_eq!(web.browse(|| with_headers("https://example.org/gone", &[])).unwrap_err().to_string(), said);
    }

    #[cfg(unix)]
    #[test]
    fn text_is_a_body_in_utf_8_and_anything_else_is_refused() {
        let web = testing::Web::new();
        web.serves("https://example.org/t", &[], "héllo".as_bytes())
            .serves("https://example.org/b", &[], &[0xff, 0xfe]);
        assert_eq!(web.browse(|| text("https://example.org/t", &[])).ok().as_deref(), Some("héllo"));
        let e = web.browse(|| text("https://example.org/b", &[])).unwrap_err().to_string();
        assert_eq!(e, "https://example.org/b isn't text");
    }

    #[cfg(unix)]
    #[test]
    fn the_headers_and_the_body_come_apart_at_the_blank_line() {
        let web = testing::Web::new();
        web.serves("https://example.org/m", &["Docker-Content-Digest: sha256:ab"], b"{\"a\":1}\r\n\r\nstill body");
        let (head, body) = web.browse(|| with_headers("https://example.org/m", &[])).unwrap();
        assert_eq!(header(&head, "docker-content-digest"), Some("sha256:ab"));
        assert_eq!(body, b"{\"a\":1}\r\n\r\nstill body");
        assert_eq!(web.calls()[0].get(9).map(String::as_str), Some("--include"));
    }
}
