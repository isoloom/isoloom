//! The target guides against the targets: a guide never says a runner delivers what its target
//! refuses.

use isoloom_plan::Feature;
use isoloom_spec::Target;

use crate::testing::Outcome;

/// The guide of the targets in `docs/guide/targets/<page>.md`.
fn guide(page: &str) -> Outcome<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/guide/targets")
        .join(format!("{page}.md"));
    Ok(std::fs::read_to_string(path)?)
}

/// A guide's paragraphs (blank-line separated), each on one line.
fn paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n").map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).collect()
}

#[test]
fn a_guide_never_has_the_runner_deliver_secrets_its_target_refuses() -> Outcome {
    for (target, page) in [(Target::Proxmox, "proxmox"), (Target::Esxi, "esxi")] {
        let backend = isoloom_targets::backend(target).ok_or("no backend")?;
        if backend.capabilities().has(Feature::Secrets) {
            continue;
        }
        for p in paragraphs(&guide(page)?) {
            let claims = p.contains("runner") && p.contains("delivers") && p.contains("secrets");
            assert!(!claims || p.contains("refuse"), "{page}.md: {p}");
        }
    }
    Ok(())
}

/// A server's guide says what its run does with an `internet: false` network: it seals it when
/// the run manifest does, and never says such a network keeps the internet. ESXi's said so while
/// the matrix claimed the seal, then the other way round.
#[test]
fn a_server_guide_says_its_offline_networks_are_sealed_as_the_run_manifest_does() -> Outcome {
    let lab = crate::testing::lab(
        "version: 1\nname: s\nnetworks:\n  lan: { cidr: 10.20.1.0/24, internet: false }\nmachines:\n  web: { networks: { lan: 10 }, os: ubuntu@24.04, app: { steps: [{ run: web.sh }] } }\n",
    )?;
    for (target, page) in [(Target::Proxmox, "proxmox"), (Target::Esxi, "esxi")] {
        let files = isoloom_targets::generate(&lab, target)?;
        let manifest = isoloom_manifest::parse(crate::testing::file(&files, &format!(".isoloom/{page}/run.json"))?)?;
        let seal = manifest.lifecycle.seal.ok_or_else(|| format!("{page}: the run seals"))?;
        let text = guide(page)?;
        // A guide that shows how to seal by hand shows the run's own command.
        if text.contains("nftables.sealed.nft") {
            let isoloom_manifest::Run::Action { with, .. } = &seal.steps.as_slice()[0].run else {
                return Err(format!("{page}: the seal is an ssh.run").into());
            };
            let command = with
                .iter()
                .find(|(k, _)| k.as_str() == "command")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            assert!(text.contains(&format!("\"{command}\"")), "{page}.md: not the seal's command, {command}");
        }
        let paragraphs = paragraphs(&text);
        let says_sealed = paragraphs
            .iter()
            .any(|p| p.contains("seal") && (p.contains("`internet: false`") || p.contains("offline networks")));
        assert!(says_sealed, "{page}.md never says its offline networks are sealed");
        for p in &paragraphs {
            for unsaid in ["keeps the internet", "nothing seals", "nothing closes"] {
                assert!(!p.contains(unsaid), "{page}.md: {p}");
            }
        }
    }
    Ok(())
}
