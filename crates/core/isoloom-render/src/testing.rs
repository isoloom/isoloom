//! What the crate's tests share: an environment written in YAML, as the spec the plan reads and as the
//! environment a target plans.

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::{Spec, Target};

/// The environment of `yaml`, which must parse and be valid.
pub(crate) fn lab(yaml: &str) -> Lab {
    let lab = Lab::parse(yaml).unwrap_or_else(|p| panic!("the test spec parses: {p:?}"));
    assert_eq!(lab.problems(), [], "the test spec is valid");
    lab
}

/// The environment of `yaml` as a whole (every form, no target's own facts).
pub(crate) fn spec(yaml: &str) -> Spec {
    lab(yaml).spec()
}

/// The environment `target` plans for the environment of `yaml`.
pub(crate) fn env(yaml: &str, target: Target) -> Environment {
    let lab = lab(yaml);
    lab.plan(&lab.for_target(target), target, None).expect("the spec plans")
}

/// The environment `target` plans for an environment of one machine `m` (its YAML, without networks),
/// with `templates` as the project's template texts.
pub(crate) fn machine(m: &str, target: Target, templates: &[(&str, &str)]) -> Environment {
    let yaml = format!("version: 1\nname: lab\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  m: {{ networks: {{ lan: 10 }}, {m} }}\n");
    let texts = templates
        .iter()
        .filter_map(|(path, text)| Some((path.parse().ok()?, (*text).to_owned())))
        .collect();
    let lab = lab(&yaml).with_templates(texts);
    lab.plan(&lab.for_target(target), target, None).expect("the spec plans")
}

/// Each recipe step of machine `m` (as [`machine`] writes it) on `target`, as `render` renders it.
pub(crate) fn rendered(
    m: &str,
    target: Target,
    templates: &[(&str, &str)],
    render: impl Fn(&crate::configure::Configured<'_>, &isoloom_plan::configure::Resource) -> Option<String>,
) -> Vec<Option<String>> {
    let env = machine(m, target, templates);
    let c = crate::configure::Configured::of(&env, "m").expect("a recipe");
    c.recipe().resources().map(|r| render(&c, r)).collect()
}

/// A root script run by [`on_etc`]: its stand-in `/etc`, and the commands it was stood in for.
pub(crate) struct Ran {
    dir: tempfile::TempDir,
}

impl Ran {
    /// The file at `path` under the stand-in `/etc`, empty when there's none.
    pub(crate) fn etc(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.path().join("etc").join(path)).unwrap_or_default()
    }

    /// The stood-in commands' calls, in order (`systemctl daemon-reload`), the stand-in `/etc`
    /// written `$ETC`.
    pub(crate) fn calls(&self) -> Vec<String> {
        let etc = self.dir.path().join("etc").display().to_string();
        let calls = std::fs::read_to_string(self.dir.path().join("calls")).unwrap_or_default();
        calls.lines().map(|l| l.replace(&etc, "$ETC")).collect()
    }
}

/// Root script `sh` run by `sh` on a stand-in `/etc` holding the folders `dirs` and the files
/// `files` (path, text), with `systemctl` and `sysctl` stood in for: each records its call, and
/// `systemctl is-active` answers yes for the units in `active` alone. `None` without a POSIX sh.
pub(crate) fn on_etc(sh: &str, dirs: &[&str], files: &[(&str, &str)], active: &[&str]) -> Option<Ran> {
    let mut posix = crate::test_support::posix_sh()?;
    let dir = tempfile::Builder::new().prefix("isoloom-etc-").tempdir().expect("a temporary folder");
    let etc = dir.path().join("etc");
    std::fs::create_dir_all(&etc).expect("a folder");
    for sub in dirs {
        std::fs::create_dir_all(etc.join(sub)).expect("a folder");
    }
    for (path, text) in files {
        std::fs::write(etc.join(path), text).expect("a file");
    }
    let calls = crate::test_support::sh_path(&dir.path().join("calls"));
    let active: String = active.iter().map(|u| [" ", u, " "].concat()).collect();
    let shim = format!(
        "systemctl() {{ printf 'systemctl %s\\n' \"$*\" >> {calls}; case \"$1\" in is-active) for u; do :; done; case '{active}' in *\" $u \"*) true ;; *) false ;; esac ;; esac; }}\nsysctl() {{ printf 'sysctl %s\\n' \"$*\" >> {calls}; }}\n"
    );
    let script = sh.replace("/etc/", &format!("{}/", crate::test_support::sh_path(&etc)));
    let out = posix.arg("-c").arg(format!("{shim}{script}")).output().expect("sh runs");
    assert!(out.status.success(), "{}\n{script}", String::from_utf8_lossy(&out.stderr));
    Some(Ran { dir })
}
