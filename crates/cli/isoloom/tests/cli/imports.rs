//! `isoloom import` through the binary: the draft it writes passes `isoloom validate`, and the
//! notes say what the source had that the draft doesn't.

use crate::testing::{Outcome, isoloom, scratch};

#[test]
fn a_compose_import_writes_a_draft_that_validates() -> Outcome {
    let dir = scratch("import-compose")?;
    std::fs::write(
        dir.join("compose.yaml"),
        "services:\n  web:\n    image: nginx:1.27\n    ports: [\"8080:80\"]\n    environment: { TOKEN: \"${TOKEN}\", MODE: prod }\n  cache:\n    image: redis:7\n    expose: [6379]\n",
    )?;
    let imported = isoloom(&dir, &[&"import", &"compose", &dir.join("compose.yaml")])?;
    assert_eq!(imported.code, Some(0), "{imported:?}");
    assert!(
        imported.stdout.contains("✓ wrote ") && imported.stdout.contains("\nBelongs in the image (1)\n"),
        "{imported:?}"
    );
    assert!(imported.stdout.ends_with("\n✓ the draft is valid\n"), "{imported:?}");
    let validated = isoloom(&dir, &[&"validate", &dir])?;
    assert_eq!(validated.code, Some(0), "{validated:?}");
    let again = isoloom(&dir, &[&"import", &"compose", &dir.join("compose.yaml")])?;
    assert_eq!(again.code, Some(2), "an existing spec isn't replaced: {again:?}");
    assert!(again.stderr.contains("already has a spec; use --force to replace it, or --stdout"), "{again:?}");
    Ok(())
}

#[test]
fn a_terraform_import_prints_the_draft_and_its_notes_apart() -> Outcome {
    let dir = scratch("import-terraform")?;
    std::fs::write(
        dir.join("main.tf"),
        "resource \"aws_subnet\" \"lab\" {\n  cidr_block = \"10.4.1.0/24\"\n}\nresource \"aws_instance\" \"web\" {\n  instance_type = \"t3.small\"\n  private_ip = \"10.4.1.10\"\n}\n",
    )?;
    let printed = isoloom(&dir, &[&"import", &"terraform", &dir, &"--stdout"])?;
    assert_eq!(printed.code, Some(0), "{printed:?}");
    assert!(
        printed.stdout.contains("machines:\n  web:\n    networks: { lab: 10 }\n"),
        "the draft on stdout: {printed:?}"
    );
    assert!(printed.stderr.ends_with("\n✓ the draft is valid\n"), "the notes on stderr: {printed:?}");
    std::fs::write(dir.join("isoloom.yml"), &printed.stdout)?;
    let validated = isoloom(&dir, &[&"validate", &dir])?;
    assert_eq!(validated.code, Some(0), "{validated:?}");
    Ok(())
}
