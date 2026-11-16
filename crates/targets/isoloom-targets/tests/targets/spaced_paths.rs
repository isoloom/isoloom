//! A project path may hold a space (`provision/my setup.sh`): every place a generated file names
//! one, it is one word, quoted for the shell or in a Dockerfile's JSON form.

use isoloom_spec::Target;

use crate::testing::{Outcome, check_line, define, generated, vagrantfile, valid};

const SPACED: &str = r#"
version: 1
name: spaced
networks: { app: { cidr: 10.80.0.0/24 } }
machines:
  web:
    os: debian@12
    networks: { app: 10 }
    app:
      steps:
        - { run: "provision/my setup.sh", creates: "/srv/my done" }
        - { copy: { from: "site files/", to: /srv/site/ } }
checks:
  - "checks/a b.sh"
"#;

#[test]
fn a_dockerfile_copies_and_runs_a_spaced_path_as_one_word() -> Outcome {
    let d = generated(&valid(SPACED)?, Target::Docker, "build/web/Dockerfile")?;
    let instructions: Vec<&str> = d.lines().filter(|l| !l.starts_with('#')).collect();
    for instruction in [
        "COPY [\"provision/my setup.sh\", \"provision/my setup.sh\"]",
        "RUN [ -e '/srv/my done' ] || sh 'provision/my setup.sh'",
        "COPY [\"site files/\", \"/srv/site/\"]",
    ] {
        assert!(instructions.contains(&instruction), "{instruction}: {d}");
    }
    assert!(!instructions.iter().any(|i| i.starts_with("COPY provision")), "{d}");
    Ok(())
}

#[test]
fn a_check_script_with_a_space_runs_quoted_everywhere() -> Outcome {
    let lab = valid(SPACED)?;
    let runner = generated(&lab, Target::Docker, "docker/checks/isoloom-networks.sh")?;
    let check = check_line(&runner, "checks/a b.sh")?;
    assert!(check.contains(r"cd /isoloom/project && sh '\''checks/a b.sh'\''"), "{check}");
    let vf = vagrantfile(&lab)?;
    let statements = vf.every_statement();
    assert!(
        statements
            .iter()
            .any(|s| s.contains(r#"["checks/a b.sh", "'/opt/isoloom/checks'", "'/opt/isoloom/checks/a b.sh'"]"#)),
        "{statements:?}"
    );
    let step = define(&vf, "web").statements("m.vm.provision \"shell\", name: \"provision/my setup.sh\"");
    assert!(
        matches!(step.as_slice(), [one] if one.contains(r"sh -c 'sh '\\''provision/my setup.sh'\\'''")),
        "{step:?}"
    );
    Ok(())
}
