//! One rule says which targets can run an environment: `Lab::possible` keeps the targets
//! `Lab::not_possible` rules nothing out for, and the generator refuses exactly the others as not
//! possible, on every example and on a spec whose access machine the runner supplies (the image
//! table decides its forms). `isoloom_spec::effective` once answered from the spec's forms alone,
//! skipping the access machine and the image table, so an embedder offered targets the generator
//! then refused.

use isoloom_spec::images::Table;
use isoloom_spec::model::Target;
use isoloom_targets::{Error, GenerateError, Lab, NotPossible, generate};

use crate::testing::{Outcome, examples, lab};

/// `possible`, `not_possible` and the generator agree on `lab`, for every target.
fn agree(lab: &Lab, what: &str) -> Outcome {
    let possible = lab.possible();
    for t in Target::ALL {
        let why = lab.not_possible(t);
        assert_eq!(possible.contains(&t), why.is_none(), "{what}: {}: {why:?}", t.id());
        // A target that isn't possible is refused (as not possible, or at the field lowering rules
        // out); a possible one may still be refused by its generator, never as not possible.
        match (generate(lab, t), why) {
            (Err(Error::Generate(e)), Some(_)) => assert!(e.is_refusal(), "{what}: {}: {e}", t.id()),
            (Ok(_), Some(why)) => return Err(format!("{what}: {} is generated, yet {why}", t.id()).into()),
            (Err(Error::Generate(GenerateError::NotPossible(_))), None) => {
                return Err(format!("{what}: the generator says {} isn't possible, the lab says it is", t.id()).into());
            }
            (Err(Error::Generate(_)) | Ok(_), None) => {}
            (Err(e), _) => return Err(format!("{what}: {}: {e}", t.id()).into()),
        }
    }
    Ok(())
}

#[test]
fn every_example_has_one_answer_to_which_targets_can_run_it() -> Outcome {
    let mut dirs: Vec<_> = std::fs::read_dir(examples())?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.retain(|d| d.join("isoloom.yml").is_file());
    for dir in &dirs {
        agree(&Lab::new(isoloom_spec::load(dir)?), &dir.display().to_string())?;
    }
    Ok(())
}

#[test]
fn an_access_machine_the_runner_supplies_counts_as_the_image_table_gives_it() -> Outcome {
    let spec = "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lan: 10 }, app: { steps: [{ run: a.sh }] } }\n  me: { access: true, networks: { lan: 20 } }\n";
    let bare = lab(spec)?;
    agree(&bare, "no image table")?;
    // Without an image table, every target that creates the machines is ruled out at the access
    // machine's `os`, and says so.
    assert!(
        bare.possible().iter().all(|t| matches!(t, Target::External | Target::CloudServices)),
        "{:?}",
        bare.possible()
    );
    match bare.not_possible(Target::Docker) {
        Some(NotPossible::Unsupplied(p)) => assert_eq!(p.path.as_str(), "machines.me.os"),
        other => return Err(format!("{other:?}").into()),
    }
    let table = Table::parse("access: { vm: debian@12 }\n")?;
    let vms = lab(spec)?.with_images(table);
    agree(&vms, "a VM for the access machine")?;
    assert!(
        vms.possible().contains(&Target::Vagrant) && !vms.possible().contains(&Target::Docker),
        "{:?}",
        vms.possible()
    );
    Ok(())
}
