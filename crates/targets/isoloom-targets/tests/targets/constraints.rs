//! `constraints:`: a customer's proxy, registry and package mirrors and time servers, applied on
//! every target, and nothing changed without them.

use isoloom_render::constraints::rewrite_image;
use isoloom_render::container::UTILITY_IMAGE;
use isoloom_spec::model::Constraints;
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Lab, generate};

use crate::testing::{Outcome, between, examples, generated as file, valid};

const CONSTRAINTS: &str = "
constraints:
  proxy: { http: \"http://proxy.corp:3128\", no_proxy: [.corp] }
  registries: { docker.io: registry.corp/dockerhub, ghcr.io: registry.corp/ghcr }
  mirrors: { debian: \"http://mirror.corp/debian\", pypi: \"https://pypi.corp/simple\" }
  ntp: [ntp.corp]
";

/// The segmented example (a built web image, a pulled cache image, Isoloom's router and
/// checks), with CONSTRAINTS.
fn constrained() -> Outcome<Lab> {
    let text = std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?;
    valid(&format!("{text}{CONSTRAINTS}"))
}

/// The segmented example's spec as written.
fn segmented_text() -> Outcome<String> {
    Ok(std::fs::read_to_string(examples().join("segmented/isoloom.yml"))?)
}

/// The segmented example with `extra` appended to its spec, unchecked.
fn segmented_with(extra: &str) -> Outcome<Lab> {
    Ok(Lab::new(parse(&format!("{}{extra}", segmented_text()?))?))
}

/// Constraints with these registry mirrors only.
fn mirrors(registries: &[(&str, &str)]) -> Constraints {
    let mut c = Constraints::default();
    c.registries = registries.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    c
}

#[test]
fn images_go_through_their_registry_mirror() {
    let c = mirrors(&[("docker.io", "registry.corp/dockerhub/"), ("ghcr.io", "registry.corp/ghcr")]);
    assert_eq!(rewrite_image(&c, "redis:7"), "registry.corp/dockerhub/library/redis:7");
    assert_eq!(rewrite_image(&c, "curlimages/curl:8.11.1"), "registry.corp/dockerhub/curlimages/curl:8.11.1");
    assert_eq!(rewrite_image(&c, "docker.io/library/nginx"), "registry.corp/dockerhub/library/nginx");
    assert_eq!(rewrite_image(&c, "index.docker.io/x/y@sha256:ab"), "registry.corp/dockerhub/x/y@sha256:ab");
    assert_eq!(rewrite_image(&c, "ghcr.io/org/app:1"), "registry.corp/ghcr/org/app:1");
    assert_eq!(rewrite_image(&c, "quay.io/org/app"), "quay.io/org/app", "no mirror for quay.io");
    assert_eq!(rewrite_image(&c, "localhost:5000/app"), "localhost:5000/app");
}

#[test]
fn compose_pulls_through_the_mirror_and_machines_get_the_proxy() -> Outcome {
    let compose = file(&constrained()?, Target::Docker, "docker/compose.yml")?;
    assert!(compose.contains("image: registry.corp/dockerhub/library/redis:7-alpine"), "the spec's images");
    assert!(
        compose.contains(&format!("image: registry.corp/dockerhub/library/{UTILITY_IMAGE}")),
        "Isoloom's own"
    );
    assert!(compose.contains("image: localhost/isoloom/segmented-web"), "a built image is local, not pulled");
    assert!(!compose.contains("image: redis:7-alpine") && !compose.contains(&format!("image: {UTILITY_IMAGE}")));
    // The built machine: proxy and mirrors as build arguments and environment.
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&compose).unwrap();
    let web = &doc["services"]["web"];
    assert_eq!(web["build"]["args"]["HTTP_PROXY"].as_str(), Some("http://proxy.corp:3128"));
    assert_eq!(
        web["environment"]["https_proxy"].as_str(),
        Some("http://proxy.corp:3128"),
        "https defaults to http"
    );
    assert_eq!(web["environment"]["PIP_INDEX_URL"].as_str(), Some("https://pypi.corp/simple"));
    let np = web["environment"]["NO_PROXY"].as_str().unwrap();
    for direct in ["localhost", "cache", "web", "10.61.10.0/24", ".corp"] {
        assert!(np.split(',').any(|x| x == direct), "{direct} reached directly: {np}");
    }
    // Isoloom's checks never go through the customer's proxy.
    for (name, svc) in doc["services"].as_mapping().unwrap() {
        let name = name.as_str().unwrap();
        if name.starts_with("isoloom-") {
            assert!(svc["environment"]["HTTP_PROXY"].is_null(), "{name} has no proxy");
        }
    }
    Ok(())
}

#[test]
fn kubernetes_pulls_through_the_mirror_and_containers_get_the_proxy() -> Outcome {
    let spec = constrained()?;
    let all: String = generate(&spec, Target::Kubernetes).unwrap().into_iter().map(|f| f.contents).collect();
    assert!(all.contains("image: registry.corp/dockerhub/library/redis:7-alpine"));
    assert!(all.contains("image: localhost/isoloom/segmented-web"));
    assert!(all.contains("name: HTTP_PROXY") && all.contains("value: http://proxy.corp:3128"), "{all}");
    Ok(())
}

#[test]
fn every_linux_vm_runs_the_preamble_first() -> Outcome {
    let spec = constrained()?;
    let vf = file(&spec, Target::Vagrant, "vagrant/Vagrantfile")?;
    // cache, web, user and the router: each VM's first provisioner.
    assert_eq!(vf.matches("name: \"constraints\"").count(), 4, "{vf}");
    for block in vf.split("config.vm.define").skip(1) {
        // After UTM's default-route provisioner (a no-op elsewhere), which comes first everywhere,
        // and the addresses' (NetworkManager made to keep them before its DHCP times out): neither
        // downloads anything.
        let first = block
            .lines()
            .filter(|l| l.contains(".vm.provision"))
            .find(|l| !l.contains("\"nat-route\"") && !l.contains("\"addresses\""))
            .unwrap();
        assert!(first.contains("name: \"constraints\""), "first provisioner: {first}");
    }
    assert!(vf.contains("Acquire::http::Proxy") && vf.contains("mirror.corp/debian") && vf.contains("ntp.corp") && vf.contains("timesyncd"));

    let pve = file(&spec, Target::Proxmox, "proxmox/main.tf")?;
    assert_eq!(
        pve.matches("\"sh -c 'set -e\\n# The proxy").count(),
        pve.matches("#cloud-config").count(),
        "every cloud-config"
    );

    // On the clouds, the `Prelude` step is the first of every VM's own set-up (its user data),
    // once the project is in.
    for cloud in ["aws", "azure", "gcp"] {
        let tf = file(&spec, Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?;
        let set_ups = tf.matches("<<'ISOLOOM_SETUP'").count();
        assert!(set_ups > 0, "{cloud}: {tf}");
        assert_eq!(tf.matches("running: system constraints").count(), set_ups, "{cloud}: {tf}");
        assert_eq!(tf.matches("{ sh -c 'set -e\\n# The proxy").count(), set_ups, "{cloud}: {tf}");
    }
    // The cloud Docker host sets itself up from its shared setup.sh (over SSH, the run manifest),
    // not from Terraform: the prelude leads it.
    for cloud in ["aws", "azure", "gcp", "digitalocean", "linode", "oci"] {
        let setup = file(&spec, Target::CloudDocker, &format!("cloud-docker/{cloud}/setup.sh"))?;
        assert!(
            setup.contains("# The proxy") && setup.contains("mirror.corp/debian"),
            "the Docker host on {cloud}: {setup}"
        );
    }
    // On Proxmox, before cloud-init installs the guest agent (through the proxy); Docker and the
    // rest run from setup.sh over SSH, where the prelude leads too.
    let pve_docker = file(&spec, Target::DockerVm, "docker-vm/proxmox/main.tf")?;
    assert!(pve_docker.contains("bootcmd = [\n        \"sh -c 'set -e\\n# The proxy"), "{pve_docker}");
    let setup = file(&spec, Target::DockerVm, "docker-vm/proxmox/setup.sh")?;
    assert!(setup.contains("# The proxy") && setup.contains("get.docker.com"), "{setup}");
    let docker_vm = file(&spec, Target::DockerVm, "docker-vm/Vagrantfile")?;
    let first = docker_vm
        .lines()
        .filter(|l| l.contains("config.vm.provision"))
        .find(|l| !l.contains("\"nat-route\""))
        .unwrap();
    assert!(first.contains("\"constraints\", type: \"shell\""), "{first}");
    Ok(())
}

#[test]
fn without_constraints_nothing_changes() {
    let dir = crate::testing::examples().join("segmented");
    let source = load(&dir).unwrap();
    let mut with_empty = source.clone();
    with_empty.constraints = Some(Constraints::default());
    let (plain, empty) = (Lab::new(source), Lab::new(with_empty));
    for t in [Target::Docker, Target::Kubernetes, Target::Vagrant, Target::Proxmox, Target::CloudVm] {
        let a: Vec<_> = generate(&plain, t)
            .unwrap()
            .into_iter()
            .filter(|f| !f.path.ends_with("resolved.json"))
            .collect();
        let b: Vec<_> = generate(&empty, t)
            .unwrap()
            .into_iter()
            .filter(|f| !f.path.ends_with("resolved.json"))
            .collect();
        assert_eq!(a, b, "{t:?}");
    }
}

#[test]
fn constraints_are_validated() -> Outcome {
    let lab = segmented_with(
        "
constraints:
  proxy: { http: proxy.corp:3128, no_proxy: [\"a, b\"] }
  registries: { \"https://docker.io\": \"https://registry.corp\" }
  mirrors: { npm: registry.npmjs.org }
  ntp: [\"\"]
",
    )?;
    let at: Vec<String> = lab.problems().into_iter().map(|p| p.path.to_string()).collect();
    for expected in [
        "constraints.proxy.http",
        "constraints.proxy.no_proxy[0]",
        "constraints.registries.https://docker.io",
        "constraints.mirrors.npm",
        "constraints.ntp[0]",
    ] {
        assert!(at.iter().any(|a| a == expected), "{expected} in {at:?}");
    }
    let e = parse(&format!("{}\nconstraints: {{ proxi: {{}} }}\n", segmented_text()?))
        .unwrap_err()
        .to_string();
    assert!(e.contains("proxi"), "{e}");
    Ok(())
}

/// segmented with the site's certificate authority, `certs/corp-ca.pem`.
fn with_ca() -> Outcome<Lab> {
    example_with_ca("segmented")
}

#[test]
fn every_vm_trusts_the_sites_certificate_authority_once_the_project_is_in() -> Outcome {
    let lab = with_ca()?;
    let vf = file(&lab, Target::Vagrant, "vagrant/Vagrantfile")?;
    // cache, web and user: each spec VM, from the unpacked project, after it and before its
    // steps; and the router, before it installs nftables.
    assert_eq!(vf.matches("name: \"certificate authorities\"").count(), 4, "{vf}");
    assert!(
        vf.contains("install -m 0644 '/opt/isoloom/certs/corp-ca.pem' /usr/local/share/ca-certificates/isoloom-0.crt"),
        "{vf}"
    );
    assert!(vf.contains("update-ca-certificates") && vf.contains("update-ca-trust extract"), "{vf}");
    for block in vf.split("config.vm.define").skip(1).filter(|b| b.contains("certificate authorities")) {
        let at = |name: &str| block.find(&format!("name: \"{name}\"")).unwrap_or(usize::MAX);
        assert!(at("project") < at("certificate authorities"), "{block}");
    }
    for (target, path) in [(Target::Proxmox, "proxmox/main.tf"), (Target::CloudVm, "cloud-vm/aws/main.tf")] {
        let tf = file(&lab, target, path)?;
        assert!(tf.contains("/opt/isoloom/certs/corp-ca.pem"), "{target:?}: {tf}");
    }
    Ok(())
}

/// `text` has each of `names`, each after the one before it.
fn in_order(text: &str, names: &[&str]) {
    let at: Vec<Option<usize>> = names.iter().map(|n| text.find(n)).collect();
    assert!(at.iter().all(Option::is_some), "{names:?} not all in:\n{text}");
    assert!(at.is_sorted(), "{names:?} out of order in:\n{text}");
}

/// `example` with the site's certificate authority, `certs/corp-ca.pem`.
fn example_with_ca(example: &str) -> Outcome<Lab> {
    let text = std::fs::read_to_string(examples().join(example).join("isoloom.yml"))?;
    valid(&format!("{text}\nconstraints:\n  ca_certificates: [certs/corp-ca.pem]\n"))
}

#[test]
fn isoloom_s_own_vms_trust_the_certificate_authority_before_they_install_anything() -> Outcome {
    const TRUST: &str = "/opt/isoloom/certs/corp-ca.pem";
    let lab = with_ca()?;
    let vf = file(&lab, Target::Vagrant, "vagrant/Vagrantfile")?;
    let router = between(&vf, "define \"isoloom-router\"", "config.vm.define")?;
    in_order(router, &["name: \"project\"", TRUST, "apt-get install -y -qq nftables"]);
    // The containers' Docker host: on Vagrant, and as the set-up script a cloud's or Proxmox's
    // run manifest pipes in once the project is copied.
    let docker_vm = file(&lab, Target::DockerVm, "docker-vm/Vagrantfile")?;
    in_order(&docker_vm, &["\"project\", type: \"isoloom_project\"", TRUST, "download.docker.com"]);
    let setup = file(&lab, Target::CloudDocker, "cloud-docker/aws/setup.sh")?;
    in_order(&setup, &[TRUST, "download.docker.com"]);
    let hybrid = file(&example_with_ca("mixed-office")?, Target::Hybrid, "hybrid/Vagrantfile")?;
    in_order(&hybrid, &["\"isoloom_project\", name: \"project\"", TRUST, "download.docker.com"]);
    // The controller, before Ansible: on Vagrant and on the clouds.
    let paired = example_with_ca("ansible-pair")?;
    let vagrant = file(&paired, Target::Vagrant, "vagrant/Vagrantfile")?;
    in_order(
        between(&vagrant, "define \"isoloom-controller\"", "\nend\n")?,
        &["name: \"project\"", TRUST, "python3"],
    );
    let aws = file(&paired, Target::CloudVm, "cloud-vm/aws/main.tf")?;
    in_order(
        between(&aws, "resource \"aws_instance\" \"isoloom_controller\"", "\n}\n")?,
        &["project-delivered", TRUST, "python3"],
    );
    let proxmox = file(&paired, Target::Proxmox, "proxmox/main.tf")?;
    in_order(between(&proxmox, "The controller: runs", "\n}\n")?, &[TRUST, "installing Ansible"]);
    let esxi = file(&paired, Target::Esxi, "esxi/main.tf")?;
    in_order(
        between(&esxi, "resource \"terraform_data\" \"isoloom_controller\"", "\n}\n")?,
        &[TRUST, "python3 -m venv"],
    );
    Ok(())
}

#[test]
fn the_server_routers_trust_the_certificate_authority_before_they_install_their_packages() -> Outcome {
    // cloud-init's `ca_certs` runs before its `packages`: the router trusts the file from the
    // project before apt downloads.
    const AUTHORITY: &str = "ca_certs = { trusted = [file(\"${local.root}/certs/corp-ca.pem\")] }";
    let lab = example_with_ca("segmented")?;
    for (target, module, router) in [
        (
            Target::Proxmox,
            "proxmox/main.tf",
            "resource \"proxmox_virtual_environment_file\" \"isoloom_router\"",
        ),
        (Target::Esxi, "esxi/main.tf", "resource \"esxi_guest\" \"isoloom_router\""),
    ] {
        let tf = file(&lab, target, module)?;
        let config = between(&tf, router, "\n}\n")?;
        in_order(config, &[AUTHORITY, "packages"]);
    }
    Ok(())
}

#[test]
fn a_windows_vm_imports_the_certificate_authority_into_its_root_store() -> Outcome {
    let lab = example_with_ca("windows-hello")?;
    let vf = file(&lab, Target::Vagrant, "vagrant/Vagrantfile")?;
    assert!(
        vf.contains("Import-Certificate -FilePath 'C:\\\\isoloom\\\\certs\\\\corp-ca.pem' -CertStoreLocation Cert:\\\\LocalMachine\\\\Root"),
        "{vf}"
    );
    Ok(())
}

#[test]
fn containers_have_the_certificate_authority_and_built_images_trust_it() -> Outcome {
    let lab = with_ca()?;
    let compose = file(&lab, Target::Docker, "docker/compose.yml")?;
    assert!(compose.contains("file: ../../certs/corp-ca.pem"), "{compose}");
    assert_eq!(
        compose.matches("target: /usr/local/share/ca-certificates/isoloom-0.crt").count(),
        3,
        "{compose}"
    );
    let k8s = file(&lab, Target::Kubernetes, "kubernetes/environment.yaml")?;
    assert!(k8s.contains("/usr/local/share/ca-certificates/isoloom-0.crt"), "{k8s}");
    // An image Isoloom builds from steps trusts it at build time, before any step downloads.
    let steps = valid(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lan: 10 }, app: { steps: [{ packages: [curl] }] } }\nconstraints:\n  ca_certificates: [corp.pem]\n",
    )?;
    let dockerfile = file(&steps, Target::Docker, "docker/build/web/Dockerfile")?;
    let (copy, run) = (
        dockerfile.find("COPY [\"corp.pem\", \"/tmp/isoloom-ca/0.pem\"]").ok_or("no COPY")?,
        dockerfile.find("update-ca-certificates").ok_or("no RUN")?,
    );
    let install = dockerfile.find("curl").ok_or("no install")?;
    assert!(
        dockerfile.find("FROM").unwrap_or(usize::MAX) < copy && copy < run && run < install,
        "{dockerfile}"
    );
    Ok(())
}

#[test]
fn a_certificate_authority_must_be_a_file_of_the_project() -> Outcome {
    let dir = crate::testing::examples().join("segmented");
    let text = std::fs::read_to_string(dir.join("isoloom.yml"))?;
    let lab = Lab::new(parse(&format!(
        "{text}\nconstraints:\n  ca_certificates: [certs/missing.pem, /etc/ssl/ca.pem]\n"
    ))?);
    let found: Vec<String> = lab.problems_with_files(&dir).iter().map(|p| format!("{}: {}", p.path, p.reason)).collect();
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("constraints.ca_certificates[0]") && p.contains("doesn't exist")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("constraints.ca_certificates[1]") && p.contains("inside the project")),
        "{found:?}"
    );
    Ok(())
}

/// The example `name` with the site's resolvers.
fn with_resolvers(name: &str) -> Outcome<Lab> {
    let text = std::fs::read_to_string(examples().join(name).join("isoloom.yml"))?;
    valid(&format!("{text}\nconstraints:\n  dns: [10.0.0.53, \"fd00::53\"]\n"))
}

#[test]
fn the_sites_resolvers_replace_the_public_one_everywhere() -> Outcome {
    for (example, target, path) in [
        ("segmented", Target::Proxmox, "proxmox/main.tf"),
        ("ansible-pair", Target::Proxmox, "proxmox/main.tf"),
        ("segmented", Target::Esxi, "esxi/main.tf"),
        ("ansible-pair", Target::Esxi, "esxi/main.tf"),
    ] {
        let tf = file(&with_resolvers(example)?, target, path)?;
        assert!(!tf.contains("1.1.1.1"), "{example} on {target:?}: {tf}");
        assert!(tf.contains("10.0.0.53") && tf.contains("fd00::53"), "{example} on {target:?}: {tf}");
    }
    // The router's resolver, for the containers on Proxmox, forwards their queries to each.
    let containers = valid(
        "version: 1\nname: lxc\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lab: 10 }, app: { image: nginx } }\noverrides: { proxmox: { machines: { web: { lxc: true } } } }\nconstraints:\n  dns: [10.0.0.53, \"fd00::53\"]\n",
    )?;
    let lxc = file(&containers, Target::Proxmox, "proxmox/main.tf")?;
    assert!(lxc.contains("no-resolv\\nserver=10.0.0.53\\nserver=fd00::53\\n"), "{lxc}");
    // Without them, the public one.
    let public = file(&crate::testing::example("segmented")?, Target::Proxmox, "proxmox/main.tf")?;
    assert!(public.contains("\"1.1.1.1\""), "{public}");
    Ok(())
}

#[test]
fn a_resolver_is_an_ip_address() -> Outcome {
    let lab = segmented_with("\nconstraints:\n  dns: [10.0.0.53, dns.corp]\n")?;
    let problems: Vec<String> = lab.problems().iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert_eq!(problems, ["constraints.dns[1]: `dns.corp`: a resolver's IP address, e.g. 10.0.0.53"]);
    Ok(())
}
