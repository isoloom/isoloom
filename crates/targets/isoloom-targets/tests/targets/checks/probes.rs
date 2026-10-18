//! The runner scripts' probes against real servers on 127.0.0.1, run by the runner's `sh`: a
//! greeting read, a line sent, a redirect followed. Unix only, as the runner's shell is.

use isoloom_plan::checks::{self, Line, Position, Probe};

use super::{BASE, runner};
use crate::testing::{Outcome, lab};

/// A server on 127.0.0.1 answering each connection with `answer` (of the line it read, when
/// `wait_line`) after `delay_ms`, then holding the connection open a while (as a service does).
fn serve(wait_line: bool, delay_ms: u64, answer: fn(&str) -> String) -> Outcome<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            // A connection that drops is the check's to report, not the server's.
            std::thread::spawn(move || answer_one(c, wait_line, delay_ms, answer).ok());
        }
    });
    Ok(port)
}

/// One connection of [`serve`]: the line read when `wait_line`, the answer after `delay_ms`,
/// then the connection held open as a service holds it.
fn answer_one(mut c: std::net::TcpStream, wait_line: bool, delay_ms: u64, answer: fn(&str) -> String) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Write};
    let mut line = String::new();
    if wait_line {
        BufReader::new(c.try_clone()?).read_line(&mut line)?;
    }
    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    c.write_all(answer(line.trim_end_matches('\n')).as_bytes())?;
    std::thread::sleep(std::time::Duration::from_secs(8));
    Ok(())
}

#[test]
fn tcp_checks_read_a_greeting_and_send_a_line() -> Outcome {
    let banner = serve(false, 0, |_| "SSH-2.0-OpenSSH_9.6\r\n".into())?;
    let late = serve(false, 1500, |_| "220 mail ESMTP ready\r\n".into())?;
    let pong = serve(true, 0, |l| if l == "PING" { "+PONG\r\n".into() } else { format!("-ERR unknown '{l}'\r\n") })?;
    let silent = serve(false, 0, |_| String::new())?;
    let spec = lab(&format!(
        "{BASE}checks:\n  \
         - {{ name: banner, tcp: '127.0.0.1:{banner}', contains: SSH-2.0 }}\n  \
         - {{ name: late, tcp: '127.0.0.1:{late}', contains: ESMTP, wait: 0 }}\n  \
         - {{ name: ping, tcp: '127.0.0.1:{pong}', send: PING, contains: +PONG }}\n  \
         - {{ name: wrong, tcp: '127.0.0.1:{pong}', send: PING, contains: HELLO, wait: 0 }}\n  \
         - {{ name: silent, tcp: '127.0.0.1:{silent}', contains: x, wait: 0 }}\n"
    ))?;
    assert_eq!(spec.problems(), vec![]);
    let plan: Vec<checks::Resolved> = checks::plan(&spec.spec()).into_iter().filter(|c| !c.derived).collect();
    let script = runner(&Position::Machine("user".into()), &plan.iter().collect::<Vec<_>>());
    assert!(script.contains(&format!("_retry 30 _tcp_read '127.0.0.1' {pong} 'PING' '+PONG'")), "{script}");
    let out = std::process::Command::new("sh").arg("-c").arg(&script).output().expect("sh runs");
    let lines: Vec<Line> = String::from_utf8_lossy(&out.stdout).lines().filter_map(checks::parse_line).collect();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    for ok in ["banner", "late", "ping"] {
        assert!(lines.contains(&Line::Pass(ok.into())), "{ok}: {text}");
    }
    assert!(
        lines.contains(&Line::Fail(
            "wrong".into(),
            format!("127.0.0.1:{pong} answered \"+PONG \", without the expected text")
        )),
        "{text}"
    );
    assert!(
        lines.contains(&Line::Fail("silent".into(), format!("nothing read from 127.0.0.1:{silent} within 5s"))),
        "{text}"
    );
    assert_eq!(lines.last(), Some(&Line::End(3, 2)), "{text}");
    Ok(())
}

/// The runner's sh, with only these tools on its PATH (plus what the runtime itself needs): a
/// runner where curl, or wget, is all there is.
fn run_with_only(script: &str, tools: &[(&str, &str)]) -> Outcome<String> {
    let first = tools.first().map_or("none", |(name, _)| *name);
    let dir = std::env::temp_dir().join(format!("isoloom-path-{}-{first}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    for t in ["sh", "sed", "head", "tail", "grep", "mktemp", "rm", "date", "sleep", "cat", "tr"] {
        let found = which(t)?.ok_or_else(|| format!("no {t} on this host"))?;
        std::os::unix::fs::symlink(found, dir.join(t))?;
    }
    for (name, target) in tools {
        std::os::unix::fs::symlink(target, dir.join(name))?;
    }
    let out = std::process::Command::new(dir.join("sh")).arg("-c").arg(script).env("PATH", &dir).output()?;
    std::fs::remove_dir_all(&dir)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A server for the redirect checks: /old redirects to /new (200); /loop redirects to itself
/// for ever; /login answers a POST with a redirect to /home, which takes GET only.
fn redirecting_server() -> Outcome<u16> {
    serve(true, 0, |l| {
        let path = l.split_whitespace().nth(1).unwrap_or("/");
        match path {
            "/old" => "HTTP/1.1 302 Found\r\nLocation: /new\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "/loop" => "HTTP/1.1 302 Found\r\nLocation: /loop\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            // A login form: a POST is answered with a redirect to a page that takes GET only.
            "/login" if l.starts_with("POST ") => "HTTP/1.1 302 Found\r\nLocation: /home\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "/home" if !l.starts_with("GET ") => "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            _ => "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nwelcome\n".into(),
        }
    })
}

/// The runner's lines for the redirect checks against `u`, as they must read (`with_body`: the
/// checks that send a body or look for text ran too).
fn expect_redirect_lines(text: &str, u: &str, with_body: bool) {
    let lines: Vec<Line> = text.lines().filter_map(checks::parse_line).collect();
    for ok in ["first", "followed"] {
        assert!(lines.contains(&Line::Pass(ok.into())), "{ok}: {text}");
    }
    let unfollowed = Line::Fail("unfollowed".into(), format!("expected HTTP 200 from {u}/old, got 302"));
    assert!(lines.contains(&unfollowed), "{text}");
    let looping = Line::Fail("looping".into(), format!("expected HTTP 200 from {u}/loop (after redirects), got 302"));
    assert!(lines.contains(&looping), "{text}");
    if with_body {
        // A POST answered with a redirect goes on as a GET, as in a browser.
        for ok in ["followed body", "login", "empty post", "unfollowed post"] {
            assert!(lines.contains(&Line::Pass(ok.into())), "{ok}: {text}");
        }
    }
}

/// Where a tool is on this host's PATH, if it is.
fn which(tool: &str) -> Outcome<Option<String>> {
    let out = std::process::Command::new("sh").arg("-c").arg(format!("command -v {tool}")).output()?;
    Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_owned()).filter(|p| !p.is_empty()))
}

#[test]
fn http_checks_follow_redirects_when_asked() -> Outcome {
    let u = format!("http://127.0.0.1:{}", redirecting_server()?);
    let spec = lab(&format!(
        "{BASE}checks:\n  \
         - {{ name: first, http: '{u}/old', expect: 302, wait: 0 }}\n  \
         - {{ name: unfollowed, http: '{u}/old', wait: 0 }}\n  \
         - {{ name: followed, http: '{u}/old', follow: true, wait: 0 }}\n  \
         - {{ name: looping, http: '{u}/loop', follow: true, wait: 0 }}\n  \
         - {{ name: followed body, http: '{u}/old', follow: true, contains: welcome, wait: 0 }}\n  \
         - {{ name: login, http: '{u}/login', method: post, body: 'user=a', follow: true, contains: welcome, wait: 0 }}\n  \
         - {{ name: empty post, http: '{u}/login', method: POST, follow: true, wait: 0 }}\n  \
         - {{ name: unfollowed post, http: '{u}/login', method: POST, body: 'user=a', expect: 302, wait: 0 }}\n"
    ))?;
    assert_eq!(spec.problems(), vec![]);
    let plan: Vec<checks::Resolved> = checks::plan(&spec.spec()).into_iter().filter(|c| !c.derived).collect();
    assert!(matches!(&plan[2].probe, Probe::Http { request, .. } if request.follow));
    let position = Position::Machine("user".into());
    let script = runner(&position, &plan.iter().collect::<Vec<_>>());
    assert!(script.contains(&format!("_retry 0 _http_is '{u}/old' 200 follow;")), "{script}");
    assert!(script.contains(&format!("-L --max-redirs 5 '{u}/old'")), "{script}");
    let expect = |text: &str, with_body: bool| expect_redirect_lines(text, &u, with_body);
    // curl, else GNU wget, else busybox's: the same answers (wget follows whatever it's told).
    if let Some(curl) = which("curl")? {
        expect(&run_with_only(&script, &[("curl", &curl)])?, true);
    }
    assert!(script.contains(&format!("-L --max-redirs 5 --data-binary 'user=a' '{u}/login'")), "{script}");
    assert!(script.contains(&format!("-L --max-redirs 5 -d '' '{u}/login'")), "{script}");
    assert!(script.contains(&format!("-X 'POST' --data-binary 'user=a' '{u}/login'")), "{script}");
    let gets: Vec<&checks::Resolved> = plan
        .iter()
        .filter(|c| matches!(&c.probe, Probe::Http { request, .. } if request.method.is_none() && request.contains.is_none()))
        .collect();
    let get_script = runner(&position, &gets);
    if let Some(gnu) = which("wget")? {
        expect(&run_with_only(&get_script, &[("wget", &gnu)])?, false);
    }
    if let Some(busybox) = which("busybox")? {
        expect(&run_with_only(&get_script, &[("wget", &busybox)])?, false);
    }
    Ok(())
}
