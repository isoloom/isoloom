//! A project path may hold a space (`provision/my setup.sh`): every place a generated file names
//! one, it is one word, quoted for the shell or in a Dockerfile's JSON form.

use isoloom_spec::Target;

use crate::testing::{Outcome, generated, valid};

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
    assert!(d.contains("COPY [\"provision/my setup.sh\", \"provision/my setup.sh\"]\n"), "{d}");
    assert!(d.contains("RUN [ -e '/srv/my done' ] || sh 'provision/my setup.sh'\n"), "{d}");
    assert!(d.contains("COPY [\"site files/\", \"/srv/site/\"]\n"), "{d}");
    assert!(!d.contains("COPY provision"), "{d}");
    Ok(())
}

#[test]
fn a_check_script_with_a_space_runs_quoted_everywhere() -> Outcome {
    let lab = valid(SPACED)?;
    let compose = generated(&lab, Target::Docker, "docker/checks/networks.sh")?;
    assert!(compose.contains(r"cd /isoloom/project && sh '\''checks/a b.sh'\''"), "{compose}");
    let vagrant = generated(&lab, Target::Vagrant, "vagrant/Vagrantfile")?;
    assert!(
        vagrant.contains(r#"["checks/a b.sh", "'/opt/isoloom/checks'", "'/opt/isoloom/checks/a b.sh'"]"#),
        "{vagrant}"
    );
    assert!(vagrant.contains(r"sh -c 'sh '\\''provision/my setup.sh'\\'''"), "{vagrant}");
    Ok(())
}
