//! A network appliance's service: the files it reads, how it starts, and its interfaces in the
//! order its image expects.

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_spec::contract::OUTPUT_DIR;

use crate::appliances::{self, Port};
use crate::file::{Attachment, Build, RO_BIND, Service};
use crate::interpolation::literal;
use crate::layout::{DIR, ROOT};
use crate::machine::Machine;
use crate::outside;

/// A network appliance's files, start and interfaces in the order it expects (nothing for a
/// machine that isn't one); its mounts and environment add to the spec's own.
pub(super) fn wire(env: &Environment, m: Machine<'_>, svc: &mut Service, files: &mut Vec<GeneratedFile>) {
    let Some((d, kind)) = m.spec.docker.as_ref().and_then(|d| Some((d, d.appliance?))) else {
        return;
    };
    let spec = env.spec();
    let name = m.name;
    let w = appliances::wiring(env, name, m.spec, kind);
    for f in w.files {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/{}", f.name),
            contents: f.contents,
        });
        svc.volumes.push(format!("./appliances/{name}/{}:{}:{RO_BIND}", f.name, f.inside).into());
    }
    if let Some(own) = &d.config {
        svc.volumes.push(format!("{ROOT}/{own}:{}:{RO_BIND}", w.own_config).into());
    }
    if let (Some(fw), Some(inside)) = (&d.firmware, w.firmware) {
        svc.volumes.push(format!("{ROOT}/{fw}:{inside}:{RO_BIND}").into());
    }
    // An emulator Isoloom builds itself (its Dockerfile next to the Compose file).
    if let Some(dockerfile) = w.build {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/appliances/{name}/build/Dockerfile"),
            contents: pull_from(env, dockerfile),
        });
        svc.image = None;
        svc.build = Some(Build {
            context: format!("./appliances/{name}/build"),
            ..Build::default()
        });
    }
    // `$$`: the script's own variables, not Compose's interpolation.
    svc.entrypoint = w.entrypoint.iter().map(|e| literal(e)).collect();
    svc.environment.get_or_insert_default().extend(w.environment);
    if w.privileged {
        svc.privileged = true;
    } else {
        svc.cap_add = vec!["NET_ADMIN".into(), "NET_RAW".into()];
    }
    let outside: Vec<(String, Option<String>)> = outside::attachments(m.plan).map(|(net, a)| (net, a.and_then(|a| a.ipv4_address))).collect();
    svc.networks = appliance_networks(spec, name, &w.ports, outside);
}

/// The interface names the image expects, set outright (Compose 2.36+, Docker 28.1+): the order
/// Docker attaches networks in isn't theirs to rely on. The existing networks it joins (`outside`,
/// each with its fixed address) come after its own, as a machine's do.
fn appliance_networks(
    spec: &isoloom_spec::model::Spec,
    name: &str,
    ports: &[Port],
    outside: Vec<(String, Option<String>)>,
) -> IndexMap<String, Option<Attachment>> {
    let mgmt = Attachment {
        interface_name: Some("eth0".into()),
        priority: Some(1000),
        ..Attachment::at(appliances::mgmt_address(spec, name))
    };
    let own = ports.iter().map(|p| (p.network.clone(), p.address.map(|a| a.to_string())));
    let mut nets = IndexMap::from([(appliances::MGMT_NETWORK.to_owned(), Some(mgmt))]);
    for (k, (network, address)) in own.chain(outside).enumerate() {
        let a = Attachment {
            ipv4_address: address,
            interface_name: Some(format!("eth{}", k + 1)),
            priority: Some(999u32.saturating_sub(u32::try_from(k).unwrap_or(u32::MAX))),
            ..Attachment::default()
        };
        nets.insert(network, Some(a));
    }
    nets
}

/// A Dockerfile's `FROM` lines through the registry mirrors.
fn pull_from(env: &Environment, dockerfile: &str) -> String {
    dockerfile
        .split_inclusive('\n')
        .map(|line| match line.strip_prefix("FROM ") {
            Some(rest) => {
                let (image, tail) = rest.find(char::is_whitespace).and_then(|at| rest.split_at_checked(at)).unwrap_or((rest, ""));
                format!("FROM {}{tail}", env.images.pull(image))
            }
            None => line.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_from_line_keeps_its_stage_name_and_other_lines_stay() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n",
        );
        let dockerfile = "FROM ubuntu:24.04 AS base\nRUN true\nFROM ubuntu:24.04\n";
        assert_eq!(pull_from(&env, dockerfile), dockerfile);
    }

    #[test]
    fn an_appliance_keeps_the_specs_mounts_and_environment_beside_its_own() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 } }\nvolumes: { data: {} }\nmachines:\n  r:\n    networks: { a: 1 }\n    volumes: { data: /data }\n    host_folders: [{ from: shared, path: /shared }]\n    environment: { SITE: lab }\n    app: { image: iol, appliance: cisco-iol }\n",
        );
        let file = crate::render::compose_file(crate::backend::ComposeTarget::Docker, &env).unwrap().file;
        let r = &file.services["r"];
        let mounts = serde_yaml_ng::to_string(&r.volumes).unwrap();
        for wanted in ["data:/data", "target: /shared", "./appliances/r/"] {
            assert!(mounts.contains(wanted), "{wanted}: {mounts}");
        }
        let environment = r.environment.as_ref().unwrap();
        assert_eq!(environment.get("SITE").map(String::as_str), Some("lab"), "{environment:?}");
        assert_eq!(environment.get("IOL_PID").map(String::as_str), Some("1"), "{environment:?}");
    }

    #[test]
    fn an_appliance_keeps_the_existing_networks_it_joins_after_its_own() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 }, corp: { cidr: 192.168.50.0/24 } }\nmachines:\n  r: { networks: { a: 1, corp: 7 }, app: { image: iol, appliance: cisco-iol } }\noverrides:\n  docker: { networks: { corp: { external: corp-net } } }\n",
        );
        let file = crate::render::compose_file(crate::backend::ComposeTarget::Docker, &env).unwrap().file;
        let nets = &file.services["r"].networks;
        assert_eq!(nets.keys().collect::<Vec<_>>(), ["isoloom-mgmt", "a", "corp"], "{nets:?}");
        let corp = nets["corp"].as_ref().unwrap();
        assert_eq!(corp.ipv4_address.as_deref(), Some("192.168.50.7"));
        assert_eq!((corp.interface_name.as_deref(), corp.priority), (Some("eth2"), Some(998)));
    }

    #[test]
    fn appliance_interfaces_attach_in_order_from_the_highest_priority() {
        let target = isoloom_spec::model::Target::Docker;
        let lab = isoloom_plan::Lab::parse(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 }, b: { cidr: 10.2.0.0/24 } }\nmachines:\n  r: { networks: { a: 1, b: 1 }, app: { image: iol, appliance: cisco-iol } }\n",
        )
        .expect("parses");
        let spec = lab.for_target(target);
        let env = lab.plan(&spec, target, None).expect("the spec plans");
        let ports = appliances::ports(&env, "r", &spec.machines["r"]);
        let nets = appliance_networks(&spec, "r", &ports, Vec::new());
        let order: Vec<(&str, Option<u32>)> = nets.iter().map(|(n, a)| (n.as_str(), a.as_ref().and_then(|a| a.priority))).collect();
        assert_eq!(order, [("isoloom-mgmt", Some(1000)), ("a", Some(999)), ("b", Some(998))]);
    }
}
