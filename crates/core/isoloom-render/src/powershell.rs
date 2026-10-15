//! PowerShell, as a Windows machine's set-up is written: string literals that hold any text,
//! and the script that unpacks the project.

/// Unpacks the project's tar.gz on Windows (`$archive`, `$dest`, `$strip` set before it): no
/// `tar.exe` before Windows Server 2019.
pub const UNTAR: &str = include_str!("powershell/untar.ps1");

/// A PowerShell single-quoted string literal reading as `s`: nothing inside is expanded.
/// PowerShell takes the typographic single quotes (U+2018 to U+201B) as quotes too, so each of
/// those is doubled as well as `'`.
#[must_use]
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if is_quote(c) {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// Whether PowerShell reads `c` as a single quote.
fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}')
}

#[cfg(test)]
mod tests {
    use super::{UNTAR, quote};

    #[test]
    fn every_kind_of_single_quote_is_doubled() {
        assert_eq!(quote("it's"), "'it''s'");
        assert_eq!(
            quote("a\u{2018}b\u{2019}c\u{201A}d\u{201B}e"),
            "'a\u{2018}\u{2018}b\u{2019}\u{2019}c\u{201A}\u{201A}d\u{201B}\u{201B}e'"
        );
        assert_eq!(quote("$x `n \""), "'$x `n \"'");
    }

    #[test]
    fn the_untar_script_reads_the_archive_it_is_given() {
        assert!(UNTAR.contains("$archive") && UNTAR.contains("$dest") && UNTAR.contains("$strip"));
    }
}
