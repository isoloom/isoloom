//! A machine's catalogue service, lowered (design §4.9): the entry's forms, ports, data and
//! configuration, and every problem and refusal at the field the author wrote.

use isoloom_spec::model::{BaseImage, Target};
use isoloom_spec::syntax::Code;

use crate::configure::ContainerProcess;
use crate::environment::{Build, Image};
use crate::testing;

/// A lab of `web`, written as `machine`, on one network beside `fpm`.
fn lab(machine: &str) -> crate::Lab {
    testing::lab(&format!(
        "version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {machine}\n  fpm: {{ os: debian@12, networks: {{ lab: 20 }} }}\n"
    ))
}

/// The problems of `web` (written as `machine`), as (code, path).
fn problems(machine: &str) -> Vec<(Code, String)> {
    lab(machine).problems().into_iter().map(|p| (p.code, p.path.to_string())).collect()
}

/// What machine `web`'s container runs on the Docker target.
fn process(l: &crate::Lab) -> ContainerProcess {
    let env = testing::planned(l, Target::Docker);
    match &env.machines["web"].image {
        Some(Image::Container {
            build: Some(Build::Generated { process, .. }),
            ..
        }) => process.clone(),
        other => panic!("not a generated container: {other:?}"),
    }
}

/// What `target` refuses of `web` (written as `machine`), as (path, reason).
fn refused(machine: &str, target: Target) -> Vec<(String, String)> {
    let l = lab(machine);
    l.lowered(crate::lower::Scope::Target(target))
        .refusals
        .into_iter()
        .map(|p| (p.path.to_string(), p.reason))
        .collect()
}

#[test]
fn a_service_on_its_images_os_runs_the_official_image_its_configuration_over_it() {
    let l = lab("{ os: debian@13, service: nginx@1.30, networks: { lab: 10 }, ports: { http: { publish: 8080 } } }");
    assert_eq!(l.problems(), []);
    let spec = l.for_target(Target::Docker);
    let web = &spec.machines["web"];
    let generated = web.docker.as_ref().and_then(|d| d.generated.clone()).expect("a generated image");
    assert_eq!(generated.from, BaseImage::Entry);
    assert!(
        generated.base.starts_with("docker.io/library/nginx:1.30.6-trixie@sha256:"),
        "{}",
        generated.base
    );
    let port = &web.ports[0];
    assert_eq!((port.name.as_deref(), port.port, port.publish, port.http), (Some("http"), 80, Some(8080), true));
    assert_eq!(process(&l), ContainerProcess::Image, "the image keeps its own entrypoint and command");
    let env = testing::planned(&l, Target::Docker);
    let recipe = env.machines["web"].recipe.as_ref().expect("a recipe");
    assert!(recipe.install.is_empty(), "the official image is the install phase");
    assert_eq!(recipe.configure.first().map(crate::configure::Resource::kind), Some("template"));
}

#[test]
fn a_service_on_another_os_is_built_from_that_os_with_the_whole_recipe() {
    let l = lab("{ os: debian@12, service: nginx, networks: { lab: 10 } }");
    let spec = l.for_target(Target::Docker);
    let generated = spec.machines["web"]
        .docker
        .as_ref()
        .and_then(|d| d.generated.clone())
        .expect("a generated image");
    assert_eq!(generated.from, BaseImage::Os);
    assert!(generated.base.starts_with("debian:12@sha256:"));
    let config = spec.machines["web"].configuration.as_ref().expect("its configuration");
    assert_eq!(
        config.install.iter().map(isoloom_spec::syntax::Step::kind).collect::<Vec<_>>(),
        ["repository", "packages"]
    );
    assert!(
        config.files.keys().any(|k| k.as_str().ends_with("default.conf")),
        "the entry's template travels with it"
    );
    let runs = process(&l);
    assert!(matches!(&runs, ContainerProcess::One(d) if d.foreground.program() == "nginx"), "{runs:?}");
    let vm = l.for_target(Target::Vagrant);
    assert_eq!(vm.machines["web"].vm.as_ref().map(|v| v.os.as_str()), Some("debian-12"));
}

#[test]
fn an_unknown_entry_a_release_or_an_os_it_lacks_and_a_setting_it_lacks_are_problems() {
    assert_eq!(
        problems("{ os: debian@12, service: nginz, networks: { lab: 10 } }"),
        [(Code::UnknownReference, "machines.web.service".to_owned())]
    );
    assert_eq!(
        problems("{ os: debian@12, service: nginx@2, networks: { lab: 10 } }"),
        [(Code::InvalidValue, "machines.web.service".to_owned())]
    );
    assert_eq!(
        problems("{ os: kali, service: nginx, networks: { lab: 10 } }"),
        [(Code::InvalidValue, "machines.web.os".to_owned())]
    );
    assert_eq!(
        problems("{ os: debian@12, service: nginx, with: { prot: 81 }, networks: { lab: 10 } }"),
        [(Code::UnknownField, "machines.web.with.prot".to_owned())]
    );
    let typo = lab("{ os: debian@12, service: nginz, networks: { lab: 10 } }").problems().remove(0);
    assert_eq!(typo.fix.as_deref(), Some("did you mean `nginx`?"));
    assert!(typo.span.is_some(), "placed at the line the author wrote");
}

#[test]
fn a_port_the_entry_lacks_is_an_unknown_reference_and_its_number_the_entry_one() {
    assert_eq!(
        problems("{ os: debian@12, service: nginx, networks: { lab: 10 }, ports: { web: { publish: true } } }"),
        [(Code::UnknownReference, "machines.web.ports.web".to_owned())]
    );
    let spec = lab("{ os: debian@12, service: nginx, with: { port: 8081 }, networks: { lab: 10 } }").spec();
    assert_eq!(spec.machines["web"].ports[0].port, 8081, "the `port` setting gives the number");
}

#[test]
fn an_endpoint_reaches_the_template_as_the_machines_address_on_each_target() {
    let spec = lab("{ os: debian@12, service: nginx, with: { php_fpm: 'fpm:9000' }, networks: { lab: 10 } }").spec();
    let config = spec.machines["web"].configuration.as_ref().expect("its configuration");
    let php = config
        .files
        .iter()
        .find(|(k, _)| k.as_str().ends_with("php.conf"))
        .map(|(_, t)| t.as_str())
        .unwrap_or_default();
    assert!(php.contains("fastcgi_pass  {{ machines.fpm.addresses.lab }}:9000;"), "{php}");
}

#[test]
fn several_services_are_refused_on_a_container_target() {
    let found = refused("{ os: debian@12, service: [nginx, nginx@1.31], networks: { lab: 10 } }", Target::Docker);
    assert_eq!(found[0].0, "machines.web.service");
    assert!(found[0].1.contains("a container runs one"), "{found:?}");
}

#[test]
fn every_form_renders_the_recipe_of_an_entry_including_a_linux_host_that_exists() {
    for (os, target) in [("debian@12", Target::Docker), ("debian@13", Target::Docker), ("debian@12", Target::Vagrant)] {
        let found = refused(&format!("{{ os: {os}, service: nginx, networks: {{ lab: 10 }} }}"), target);
        assert_eq!(found, [], "{os} on {target:?}");
    }
    let existing = testing::lab(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, service: nginx, networks: { lab: 10 } }\noverrides: { external: { machines: { web: { ssh: { address: 192.0.2.1 } } } } }\n",
    );
    let external = existing.lowered(crate::lower::Scope::Target(Target::External)).refusals;
    assert_eq!(external, [], "nginx's recipe runs on a Linux existing host through the Ansible CM tool");
}

#[test]
fn a_setting_naming_a_project_folder_it_lacks_is_a_problem_with_files() {
    let l = lab("{ os: debian@12, service: nginx, with: { root: site/ }, networks: { lab: 10 } }");
    let dir = std::env::temp_dir();
    let found: Vec<String> = l
        .problems_with_files(&dir.join("isoloom-no-such-project"))
        .iter()
        .map(|p| p.path.to_string())
        .collect();
    assert!(found.contains(&"machines.web.with.root".to_owned()), "{found:?}");
}

/// nginx's entry, its `start` a guarded command that Isoloom's runner runs before the official
/// image's own process (`image.start: run`).
fn started_catalogue() -> isoloom_catalogue::Catalogue {
    let nginx = isoloom_catalogue::Catalogue::embedded().unwrap().entry("nginx").unwrap();
    let files: Vec<(String, String)> = nginx
        .files()
        .iter()
        .map(|(path, text)| {
            let text = if path == "entry.yml" {
                text.replace(
                    "  start: []",
                    "  start:\n    - command: [touch, /run/isoloom-started]\n      creates: /run/isoloom-started",
                ) + "\nimage: { start: run }\n"
            } else {
                text.clone()
            };
            (format!("nginx/{path}"), text)
        })
        .collect();
    isoloom_catalogue::Catalogue::from_files(files.iter().map(|(p, t)| (p.as_str(), t.as_str()))).unwrap()
}

#[test]
fn an_image_whose_start_the_runner_runs_runs_it_then_its_own_process() {
    let source = isoloom_spec::syntax::parse(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@13, service: nginx, networks: { lab: 10 } }\n",
    )
    .unwrap();
    let catalogue = started_catalogue();
    let lowered = crate::lower::lower(&source, &catalogue, crate::lower::Scope::Target(Target::Docker));
    assert_eq!(lowered.refusals, [], "the runner renders the start steps");
    let env = crate::Environment::build(&lowered.spec, Target::Docker, None);
    let web = &env.machines["web"];
    let Some(Image::Container {
        build: Some(Build::Generated { process, .. }),
        ..
    }) = &web.image
    else {
        panic!("a generated build over the image")
    };
    let ContainerProcess::Started { entrypoint, command } = process else {
        panic!("{process:?}")
    };
    assert_eq!(
        (entrypoint.as_slice(), command.as_slice()),
        (
            ["/docker-entrypoint.sh".to_owned()].as_slice(),
            ["nginx".to_owned(), "-g".to_owned(), "daemon off;".to_owned()].as_slice()
        )
    );
    let recipe = web.recipe.as_ref().unwrap();
    assert!(
        recipe.install.is_empty() && recipe.start.len() == 1,
        "the image is install; its start is the runner's"
    );
}
