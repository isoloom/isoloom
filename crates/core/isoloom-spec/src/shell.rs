//! Shell quoting for the scripts Isoloom writes and runs.

/// `s` as one single-quoted POSIX sh word: `it's` becomes `'it'\''s'`, `""` becomes `''`.
#[must_use]
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A here-document's terminator for `text`: `preferred`, else `preferred_1`, `preferred_2`, … the
/// first that no line of `text` is, so nothing in `text` ends the document early. A shell's quoted
/// here-document (`<<'TAG'`) and Ruby's end at the first line that is exactly the terminator.
#[must_use]
pub fn here_tag(preferred: &str, text: &str) -> String {
    let taken = |tag: &str| text.lines().any(|line| line == tag);
    let mut tag = preferred.to_owned();
    let mut n = 0u32;
    while taken(&tag) {
        n += 1;
        tag = format!("{preferred}_{n}");
    }
    tag
}

/// `command` fed `text` through a quoted here-document: `command <<'TAG'`, `text`, then `TAG` on
/// its own line, `TAG` from [`here_tag`] so no line of `text` ends it early, and nothing in it is
/// expanded. `text` gets a final newline when it has none. Ends with a newline.
#[must_use]
pub fn heredoc(command: &str, preferred: &str, text: &str) -> String {
    let tag = here_tag(preferred, text);
    let newline = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    format!("{command} <<'{tag}'\n{text}{newline}{tag}\n")
}

/// Standard base64 (RFC 4648), padded, on one line: how a value of any bytes crosses a
/// script's standard input as one line of text.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|i| chunk.get(i).copied().unwrap_or(0));
        let sextets = [a >> 2, ((a & 0b11) << 4) | (b >> 4), ((b & 0b1111) << 2) | (c >> 6), c & 0b11_1111];
        // `n` bytes give `n + 1` symbols; padding fills the four.
        for (i, sextet) in sextets.into_iter().enumerate() {
            out.push(if i <= chunk.len() { symbol(sextet) } else { '=' });
        }
    }
    out
}

/// The base64 symbol of six bits.
fn symbol(sextet: u8) -> char {
    char::from(match sextet {
        0..=25 => b'A' + sextet,
        26..=51 => b'a' + (sextet - 26),
        52..=61 => b'0' + (sextet - 52),
        62 => b'+',
        _ => b'/',
    })
}

#[cfg(test)]
mod tests {
    use super::{base64, quote};

    #[test]
    fn base64_is_the_standard_one() {
        for (raw, enc) in [
            ("", ""),
            ("a", "YQ=="),
            ("ab", "YWI="),
            ("abc", "YWJj"),
            ("abcd", "YWJjZA=="),
            ("abcde", "YWJjZGU="),
            ("abcdef", "YWJjZGVm"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn base64_uses_every_symbol_of_its_alphabet() {
        let bytes = [
            0x00, 0x10, 0x83, 0x10, 0x51, 0x87, 0x20, 0x92, 0x8b, 0x30, 0xd3, 0x8f, 0x41, 0x14, 0x93, 0x51, 0x55, 0x97, 0x61, 0x96, 0x9b, 0x71, 0xd7, 0x9f,
            0x82, 0x18, 0xa3, 0x92, 0x59, 0xa7, 0xa2, 0x9a, 0xab, 0xb2, 0xdb, 0xaf, 0xc3, 0x1c, 0xb3, 0xd3, 0x5d, 0xb7, 0xe3, 0x9e, 0xbb, 0xf3, 0xdf, 0xbf,
        ];
        assert_eq!(base64(&bytes), "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/");
    }

    proptest::proptest! {
        #[test]
        fn base64_is_four_symbols_per_three_bytes_padded_at_the_end(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..64)) {
            let encoded = base64(&bytes);
            proptest::prop_assert_eq!(encoded.len(), bytes.len().div_ceil(3) * 4);
            let padding = encoded.chars().rev().take_while(|c| *c == '=').count();
            proptest::prop_assert_eq!(padding, (3 - bytes.len() % 3) % 3);
            proptest::prop_assert!(encoded.trim_end_matches('=').chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/'));
        }
    }

    #[test]
    fn a_heredoc_keeps_its_preferred_terminator_unless_a_line_of_the_text_is_it() {
        use super::{here_tag, heredoc};
        assert_eq!(heredoc("cat > /f", "UNIT", "a\nb\n"), "cat > /f <<'UNIT'\na\nb\nUNIT\n");
        assert_eq!(heredoc("cat > /f", "UNIT", "a"), "cat > /f <<'UNIT'\na\nUNIT\n");
        // A line that is exactly the terminator (an environment value, a user's script) would end
        // the document there and run the rest as the shell's own commands.
        assert_eq!(here_tag("COMMAND", "x\nCOMMAND\nrm -rf /\n"), "COMMAND_1");
        assert_eq!(here_tag("COMMAND", "COMMAND\nCOMMAND_1\n"), "COMMAND_2");
        assert_eq!(here_tag("COMMAND", " COMMAND\nCOMMANDS\n"), "COMMAND", "only a whole line ends it");
    }

    #[cfg(unix)]
    proptest::proptest! {
        #[test]
        fn a_heredoc_hands_its_command_the_text_exactly(lines in proptest::collection::vec("(UNIT|UNIT_1|[ -~]{0,12})", 0..6)) {
            let text: String = lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect();
            let out = std::process::Command::new("sh").arg("-c").arg(super::heredoc("cat", "UNIT", &text)).output().unwrap();
            proptest::prop_assert!(out.status.success());
            proptest::prop_assert_eq!(String::from_utf8_lossy(&out.stdout), text);
        }
    }

    /// The here-documents Isoloom's code writes around text it doesn't choose go through
    /// [`super::heredoc`] or [`super::here_tag`]; those left are around fixed text, each named
    /// here with why its terminator can't be a line of it.
    #[test]
    fn every_heredoc_around_text_isoloom_doesnt_choose_picks_its_terminator() {
        const FIXED: &[(&str, &str)] = &[
            // A systemd unit of constants.
            ("core/isoloom-render/src/steps.rs", "<<'UNIT'"),
            ("backends/isoloom-vagrant/src/vms/hyperv.rs", "<<'UNIT'"),
            // The set-up's unit, written by `unit()` from constants.
            ("backends/isoloom-terraform/src/machine/user_data.rs", "<<'ISOLOOM_UNIT'"),
            // The account-id shims a translated Dockerfile's steps run first: Isoloom's own text.
            ("core/isoloom-plan/src/dockerfile.rs", "<<'SHIM'"),
        ];
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut found = Vec::new();
        let mut dirs = vec![crates.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if path.is_dir() {
                    if !["tests", "target", "fixtures", "snapshots"].contains(&name.as_str()) {
                        dirs.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                // The code, not its tests.
                let code = text.split("#[cfg(test)]\nmod ").next().unwrap_or_default();
                let rel = path.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
                for (at, _) in code.match_indices("<<'") {
                    let opener: String = code
                        .get(at..)
                        .unwrap_or_default()
                        .chars()
                        .take_while(|c| *c != '\\' && *c != '\n' && *c != ' ')
                        .collect();
                    let picked = opener.contains('{') || opener.contains("#{");
                    if !picked && !FIXED.contains(&(rel.as_str(), opener.as_str())) && rel != "core/isoloom-spec/src/shell.rs" {
                        found.push(format!("{rel}: {opener}"));
                    }
                }
            }
        }
        assert!(
            found.is_empty(),
            "a here-document with a fixed terminator: use isoloom_spec::shell::heredoc\n{}",
            found.join("\n")
        );
    }

    #[test]
    fn a_word_is_single_quoted_its_own_quotes_closed_escaped_and_reopened() {
        for (word, quoted) in [
            ("abc", "'abc'"),
            ("", "''"),
            ("it's", "'it'\\''s'"),
            ("'", "''\\'''"),
            ("$(rm -rf /) `x` \"y\" \\n", "'$(rm -rf /) `x` \"y\" \\n'"),
        ] {
            assert_eq!(quote(word), quoted, "{word:?}");
        }
    }
}
