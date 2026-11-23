//! The words of a Dockerfile's instructions: Docker's variable substitution, paths against the
//! working folder, and the shell quoting of what the steps run.

/// `text` with `$NAME`, `${NAME}`, `${NAME:-default}` and `${NAME:+alt}` filled in from `vars`
/// (unset: empty), `\$` kept literal: Docker's substitution in instructions other than `RUN`.
pub(in crate::dockerfile) fn substitute(text: &str, vars: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('\\', Some('$')) => {
                out.push('$');
                chars.next();
            }
            ('$', Some('{')) => {
                chars.next();
                let inner: String = chars.by_ref().take_while(|ch| *ch != '}').collect();
                out.push_str(&braced(&inner, vars));
            }
            ('$', Some(n)) if n.is_ascii_alphabetic() || n == '_' => {
                let mut name = String::new();
                while let Some(ch) = chars.peek().copied().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_') {
                    name.push(ch);
                    chars.next();
                }
                out.push_str(&value(&name, vars).unwrap_or_default());
            }
            (c, _) => out.push(c),
        }
    }
    out
}

/// The value of `name` in `vars` (the last one set).
fn value(name: &str, vars: &[(String, String)]) -> Option<String> {
    vars.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

/// `${inner}` filled in: `NAME`, `NAME:-default` or `NAME:+alt`.
fn braced(inner: &str, vars: &[(String, String)]) -> String {
    let (name, op, word) = match inner.find(":-").or_else(|| inner.find(":+")) {
        Some(at) => (
            inner.get(..at).unwrap_or_default(),
            inner.get(at..at + 2).unwrap_or_default(),
            inner.get(at + 2..).unwrap_or_default(),
        ),
        None => (inner, "", ""),
    };
    let set = value(name, vars).filter(|v| !v.is_empty());
    match (op, set) {
        (":-", None) | (":+", Some(_)) => word.to_owned(),
        (":+", None) => String::new(),
        (_, v) => v.unwrap_or_default(),
    }
}

/// `path` against folder `base` (`.` is `base`), absolute.
pub(super) fn absolute(base: &str, path: &str) -> String {
    let joined = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("{}/{path}", base.trim_end_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The name part of `user[:group]`.
pub(super) fn user_name(user: &str) -> &str {
    user.split(':').next().unwrap_or(user)
}

/// `s` single-quoted for the shell.
pub(in crate::dockerfile) fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-:=@%+,".contains(c)) {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A path quoted except its glob characters, which the shell expands.
pub(super) fn glob_quote(path: &str) -> String {
    if !path.contains(['*', '?', '[']) {
        return quote(path);
    }
    let mut out = String::new();
    let mut plain = String::new();
    for c in path.chars() {
        if "*?[]".contains(c) {
            if !plain.is_empty() {
                out.push_str(&quote(&std::mem::take(&mut plain)));
            }
            out.push(c);
        } else {
            plain.push(c);
        }
    }
    if !plain.is_empty() {
        out.push_str(&quote(&plain));
    }
    out
}

/// `argv` as one shell command line.
pub(in crate::dockerfile) fn argv(a: &[String]) -> String {
    a.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use isoloom_test_support::owned_pairs;

    use super::*;

    #[test]
    fn substitution_follows_dockers_forms() {
        let v = owned_pairs(&[("A", "1"), ("EMPTY", "")]);
        assert_eq!(substitute("$A-${A}-${B:-d}-${A:+x}-${B:+x}-\\$A", &v), "1-1-d-x--$A");
        assert_eq!(substitute("${EMPTY:-d}", &v), "d");
    }

    #[test]
    fn paths_resolve_against_the_workdir() {
        assert_eq!(absolute("/var/www/html", "."), "/var/www/html");
        assert_eq!(absolute("/app", "config/x.php"), "/app/config/x.php");
        assert_eq!(absolute("/app", "/etc/../srv/"), "/srv");
    }

    #[test]
    fn globs_stay_globs() {
        assert_eq!(glob_quote("/opt/isoloom/app/*.py"), "/opt/isoloom/app/*.py");
        assert_eq!(glob_quote("/opt/isoloom/my app/*.py"), "'/opt/isoloom/my app/'*.py");
    }
}
