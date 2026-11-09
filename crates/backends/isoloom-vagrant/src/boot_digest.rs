//! What Vagrant applies to a VM only when it boots, as a digest per VM: the run manifest's
//! `boot_digest`. Vagrant sets a VM's synced folders, networks, forwarded ports, size and provider
//! settings when it boots it (`vagrant up` of a stopped VM, `vagrant reload`); `vagrant up
//! --provision` on a running VM only runs its provisioners. A runner that finds a running VM's
//! digest changed since it booted reloads it first.
//!
//! The digest is of the Vagrantfile Isoloom just wrote, the way its [`Ruby`](crate::ruby::Ruby)
//! builder lays it out: each `config.vm.define` block of the `Vagrant.configure` block, with the
//! statements of that block outside every define (what all its VMs share), but every provisioner
//! (`<var>.vm.provision ...`, its block or heredoc with it) and every comment. A Vagrantfile with
//! no define has Vagrant's one implicit machine, `default`.

use std::collections::BTreeMap;
use std::iter::Peekable;

use sha2::{Digest, Sha256};

/// Vagrant's name for the one machine of a Vagrantfile with no `config.vm.define`.
const IMPLICIT: &str = "default";

/// Each VM's boot digest (`sha256:<hex>`), by define name, of `vagrantfile`'s text.
pub(crate) fn of(vagrantfile: &str) -> BTreeMap<String, String> {
    let mut body = configure_body(vagrantfile);
    let mut shared = Vec::new();
    let mut defines: Vec<(String, Vec<&str>)> = Vec::new();
    while let Some(line) = body.next() {
        match define_name(line).filter(|_| indent(line) == 1) {
            Some(name) => defines.push((name, until_closed(&mut body, 1, "end"))),
            None => shared.push(line),
        }
    }
    let shared = boot_lines(shared);
    if defines.is_empty() {
        return BTreeMap::from([(IMPLICIT.to_owned(), digest(&shared, &[]))]);
    }
    defines.into_iter().map(|(name, own)| (name, digest(&shared, &boot_lines(own)))).collect()
}

/// The lines inside `Vagrant.configure("2") do |config|` ... its `end`.
fn configure_body(vagrantfile: &str) -> impl Iterator<Item = &str> {
    vagrantfile
        .lines()
        .skip_while(|l| !l.starts_with("Vagrant.configure("))
        .skip(1)
        .take_while(|l| *l != "end")
}

/// The lines of `lines` up to the one closing a statement at `depth` (`close` alone at that
/// depth: `end`, a heredoc's tag), which is consumed too.
fn until_closed<'a>(lines: &mut impl Iterator<Item = &'a str>, depth: usize, close: &str) -> Vec<&'a str> {
    lines.take_while(|l| l.is_empty() || indent(l) != depth || l.trim() != close).collect()
}

/// The define name of `config.vm.define "<name>"...`, if `line` is one.
fn define_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("config.vm.define \"")?;
    Some(rest.split('"').next()?.to_owned())
}

/// `lines` but blank lines, comments and provisioners (with what they span).
fn boot_lines(lines: Vec<&str>) -> Vec<&str> {
    let mut out = Vec::new();
    let mut lines = lines.into_iter().peekable();
    while let Some(line) = lines.next() {
        let code = line.trim_start();
        if code.is_empty() || code.starts_with('#') {
            continue;
        }
        if is_provisioner(code) {
            skip_spanned(&mut lines, line);
        } else {
            out.push(line);
        }
    }
    out
}

/// Whether `code` (a statement, unindented) adds a provisioner: `<var>.vm.provision ...`.
fn is_provisioner(code: &str) -> bool {
    code.split_once('.')
        .is_some_and(|(var, rest)| !var.contains(' ') && (rest.starts_with("vm.provision ") || rest == "vm.provision"))
}

/// Consumes what the statement `head` spans: the deeper lines (a block's body, a heredoc's
/// script, blank lines among them), then its `end` or its heredoc's tag.
fn skip_spanned<'a, I: Iterator<Item = &'a str>>(lines: &mut Peekable<I>, head: &str) {
    let depth = indent(head);
    while lines.next_if(|l| l.is_empty() || indent(l) > depth).is_some() {}
    let code = head.trim_end();
    let close = heredoc_tag(code).or_else(|| (code.ends_with(" do") || code.ends_with('|')).then_some("end"));
    if let Some(close) = close {
        lines.next_if(|l| indent(l) == depth && l.trim() == close);
    }
}

/// The tag of the heredoc `code` opens (`<<~'SH'`), if it opens one.
fn heredoc_tag(code: &str) -> Option<&str> {
    let (_, after) = code.rsplit_once("<<~'")?;
    after.split('\'').next()
}

/// The depth of `line`: two spaces a level, as the builder indents.
fn indent(line: &str) -> usize {
    (line.len() - line.trim_start_matches(' ').len()) / 2
}

/// `sha256:<hex>` of the shared lines, then the VM's own, each unindented to its depth.
fn digest(shared: &[&str], own: &[&str]) -> String {
    let mut h = Sha256::new();
    for l in shared.iter().chain(std::iter::once(&"--")).chain(own) {
        h.update(l.as_bytes());
        h.update(b"\n");
    }
    h.finalize().iter().fold(String::from("sha256:"), |mut out, b| {
        out.extend([b >> 4, b & 0xf].into_iter().filter_map(|n| char::from_digit(u32::from(n), 16)));
        out
    })
}

#[cfg(test)]
mod tests {
    use super::of;

    const TWO: &str = "# header
ROOT = 1
Vagrant.configure(\"2\") do |config|
  config.vm.boot_timeout = 900
  config.vm.provision \"shell\", inline: \"shared\"

  config.vm.define \"web\" do |m|
    m.vm.box = \"debian\"
    # a comment
    m.vm.provider \"utm\" do |_, o|
      o.vm.synced_folder File.expand_path(\"a\", ROOT), \"/a\", type: \"utm\", create: true
    end
    m.vm.provision \"shell\", name: \"one\", inline: <<~'SH'
      echo one

      end
    SH
    m.vm.provision \"shell\" do |s|
      s.inline = \"two\"
    end
    m.vm.provision \"checks\", type: \"shell\", run: \"never\"
  end

  config.vm.define \"db\" do |m|
    m.vm.box = \"debian\"
  end
end
";

    #[test]
    fn each_define_has_its_own_and_its_provisioners_and_comments_count_for_nothing() {
        let d = of(TWO);
        assert_eq!(d.keys().collect::<Vec<_>>(), ["db", "web"]);
        assert!(d["web"].starts_with("sha256:") && d["web"].len() == 7 + 64, "{}", d["web"]);
        assert_ne!(d["web"], d["db"]);
        let reworded = TWO
            .replace("echo one", "echo uno")
            .replace("\"two\"", "\"dos\"")
            .replace("# a comment", "# another")
            .replace("inline: \"shared\"", "inline: \"common\"");
        assert_eq!(of(&reworded), d);
        // Without its provisioners, web is db plus its shared folder.
        let bare = TWO.replace(
            "    m.vm.provider \"utm\" do |_, o|\n      o.vm.synced_folder File.expand_path(\"a\", ROOT), \"/a\", type: \"utm\", create: true\n    end\n",
            "",
        );
        assert_eq!(of(&bare)["web"], d["db"]);
    }

    #[test]
    fn a_boot_setting_changes_its_vm_and_a_shared_one_every_vm() {
        let d = of(TWO);
        let folder = of(&TWO.replace("\"/a\"", "\"/b\""));
        assert_ne!(folder["web"], d["web"]);
        assert_eq!(folder["db"], d["db"]);
        let shared = of(&TWO.replace("boot_timeout = 900", "boot_timeout = 600"));
        assert_ne!(shared["web"], d["web"]);
        assert_ne!(shared["db"], d["db"]);
    }

    #[test]
    fn a_file_without_a_define_is_vagrants_default_machine() {
        let one = "Vagrant.configure(\"2\") do |config|\n  config.vm.box = \"x\"\n  config.vm.provision \"docker\", type: \"shell\", inline: \"a\"\nend\n";
        let d = of(one);
        assert_eq!(d.keys().collect::<Vec<_>>(), ["default"]);
        assert_eq!(of(&one.replace("inline: \"a\"", "inline: \"b\"")), d);
        assert_ne!(of(&one.replace("\"x\"", "\"y\""))["default"], d["default"]);
    }
}
