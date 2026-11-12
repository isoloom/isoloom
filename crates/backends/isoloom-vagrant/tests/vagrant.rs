//! The Vagrant backend's rendering of a few examples, as snapshots, its determinism, and its
//! refusals from the plan.

use std::error::Error;
use std::path::Path;
use std::process::Command;

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_vagrant::VagrantBackend;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// An example of the project, which must be valid, planned for Vagrant.
fn plan(example: &str) -> Outcome<Environment> {
    let lab = isoloom_plan::Lab::new(isoloom_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example),
    )?);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("{example} isn't valid: {problems:?}").into());
    }
    Ok(Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?)
}

/// The files the Vagrant backend renders for an example, from its plan.
fn render(example: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(VagrantBackend.render(&plan(example)?)?)
}

/// An example's Vagrantfile.
fn vagrantfile(example: &str) -> Outcome<String> {
    let file = render(example)?
        .into_iter()
        .find(|f| f.path.ends_with("/Vagrantfile"))
        .ok_or("no Vagrantfile")?;
    Ok(file.contents)
}

/// A spec written in a test, planned for Vagrant.
fn parsed(yaml: &str) -> Outcome<Environment> {
    let lab = isoloom_plan::Lab::parse(yaml).map_err(|problems| format!("{problems:?}"))?;
    Ok(Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?)
}

#[test]
fn hello_stack() -> Outcome {
    insta::assert_snapshot!(vagrantfile("hello-stack")?);
    Ok(())
}

#[test]
fn segmented_routes_through_the_router() -> Outcome {
    insta::assert_snapshot!(vagrantfile("segmented")?);
    Ok(())
}

#[test]
fn windows_hello_over_winrm() -> Outcome {
    insta::assert_snapshot!(vagrantfile("windows-hello")?);
    Ok(())
}

#[test]
fn tuned_web_settings() -> Outcome {
    insta::assert_snapshot!(vagrantfile("tuned-web")?);
    Ok(())
}

#[test]
fn shared_files_over_nfs() -> Outcome {
    insta::assert_snapshot!(vagrantfile("shared-files")?);
    Ok(())
}

#[test]
fn ansible_pair_from_the_controller() -> Outcome {
    insta::assert_snapshot!(vagrantfile("ansible-pair")?);
    Ok(())
}

/// A VM mounting three folders of the computer that runs the environment: one of the project, one of the
/// home folder (read-only), one absolute.
const SHARED_DESK: &str = "version: 1\nname: shared-desk\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nmachines:\n  desk: { os: debian@12, networks: { lan: 10 }, host_folders: [{ from: shared, path: /srv/shared }, { from: '~/notes', path: /home/notes, read_only: true }, { from: /srv/drop, path: /srv/drop }] }\n";

#[test]
fn a_vm_shares_folders_of_the_computer_with_each_providers_own_shared_folder() -> Outcome {
    let files = VagrantBackend.render(&parsed(SHARED_DESK)?)?;
    let vagrantfile = &files.iter().find(|f| f.path.ends_with("/Vagrantfile")).ok_or("no Vagrantfile")?.contents;
    let start = vagrantfile.find("    # This computer's folders").ok_or("no host folders")?;
    let end = vagrantfile.find("    m.vm.provision \"isoloom_project\"").ok_or("no project")?;
    insta::assert_snapshot!(vagrantfile.get(start..end).ok_or("out of order")?);
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in ["hello-stack", "segmented", "edge-firewall", "vlan-office", "existing-hosts"] {
        assert_eq!(render(example)?, render(example)?, "{example}");
    }
    Ok(())
}

#[test]
fn a_renderable_example_has_no_refusal() -> Outcome {
    assert_eq!(VagrantBackend.refusal(&plan("hello-stack")?), None);
    Ok(())
}

#[test]
fn appliances_are_refused_from_the_capabilities() -> Outcome {
    let env = parsed(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  sw:\n    networks: { lab: 10 }\n    os: debian@12\n    app: { appliance: cisco-iol, image: x, steps: [{ run: s.sh }] }\n",
    )?;
    let why = VagrantBackend.refusal(&env).ok_or("not refused")?;
    assert!(why.contains("network appliance"), "{why}");
    assert!(VagrantBackend.render(&env).is_err());
    Ok(())
}

#[test]
fn a_machine_without_a_box_is_refused_without_rendering() -> Outcome {
    let lab = isoloom_plan::Lab::parse(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  m:\n    networks: { lab: 10 }\n    os: debian@12\n",
    )
    .map_err(|problems| format!("{problems:?}"))?;
    let mut spec = lab.for_target(Target::Vagrant);
    // Parsing refuses an OS it doesn't know; a spec built in code can still name one.
    spec.machines.get_mut("m").and_then(|m| m.vm.as_mut()).ok_or("a VM")?.os = "plan9".into();
    let env = Environment::build(&spec, Target::Vagrant, None)?;
    let why = VagrantBackend.refusal(&env).ok_or("not refused")?;
    assert!(why.contains("no Vagrant box for `plan9`"), "{why}");
    let refused = VagrantBackend.render(&env).err().ok_or("rendered")?;
    assert_eq!(refused.to_string(), format!("vagrant: {why}"));
    Ok(())
}

/// Vagrant 2.4 picks one build of a multi-architecture box by `box_architecture` (an older one
/// takes any: a ppc64le Debian on an amd64 host). libvirt's roboxes boxes are such boxes: a
/// machine's gets its own architecture, Isoloom's own VMs the host's.
#[test]
fn libvirts_own_boxes_are_pinned_to_an_architecture() -> Outcome {
    let file = vagrantfile("ansible-pair")?;
    let libvirt_blocks: Vec<&str> = file.split("provider \"libvirt\" do |v, o|").skip(1).collect();
    assert!(!libvirt_blocks.is_empty(), "no libvirt box of its own");
    for block in libvirt_blocks {
        let body = block.split("\n    end").next().unwrap_or_default();
        assert!(
            body.contains("o.vm.box_architecture = \"amd64\"") || body.contains("o.vm.box_architecture = :auto"),
            "{body}"
        );
    }
    assert!(file.contains("o.vm.box_architecture = :auto"), "the controller takes the host's");
    Ok(())
}

/// Hyper-V forwards no ports: each published TCP port is relayed from 127.0.0.1 to the VM at every
/// start, from the host port its `forwarded_port` line names (the runner's choice, else the
/// published one), and the relay goes with the VM. Alpine's controller keeps its hostname there:
/// setting one restarts every interface, which closed the SSH session setting it.
#[test]
fn hyperv_relays_published_ports_and_alpine_keeps_its_hostname() -> Outcome {
    let file = vagrantfile("solo-web")?;
    let host = "ISOLOOM_HOSTPORTS.fetch(\"web/web\", 8080)";
    assert!(file.contains(&format!("forwarded_port\", guest: 80, host: {host},")), "the forward");
    assert!(file.contains(&format!("IsoloomHyperV.ports(machine, [[80, {host}]])")), "the relay at start");
    assert!(file.contains(&format!("IsoloomHyperV.unports([[80, {host}]])")), "the relay gone with the VM");
    let controller = file.split("config.vm.define \"isoloom-controller\"").nth(1).ok_or("no controller")?;
    let hyperv = controller.split("provider \"hyperv\" do |v, o|").nth(1).ok_or("no hyperv block")?;
    assert!(hyperv.split("\n    end").next().unwrap_or_default().contains("o.vm.hostname = nil"));
    Ok(())
}

/// Loads a generated Vagrantfile (`ARGV[0]`) on a stand-in for Vagrant that records what it says,
/// then fires machine `ARGV[1]`'s Hyper-V triggers after `WaitForIPAddress` and `DeleteVM` on a
/// VM at 192.0.2.10, Hyper-V's PowerShell recorded instead of run. Prints each forwarded port
/// (`forward <guest> <host>`) and each PowerShell script (`ps <script>`). Vagrant's own Ruby is
/// 3.x; macOS's `ruby` is 2.6, which lacks `filter_map`, so it gets one.
const HYPERV_TRIGGERS_RB: &str = r#"
Enumerable.module_eval { def filter_map; each_with_object([]) { |x, out| v = yield(x); out << v if v }; end } unless Enumerable.method_defined?(:filter_map)
$forwards = []
$triggers = {}
$ps = []
class Rec
  def initialize(path = []); @path = path; end
  def respond_to_missing?(*); true; end
  def method_missing(name, *args, &blk)
    $machine = args[0] if name == :define
    if name == :network && args[0] == "forwarded_port"
      $forwards << [$machine, args[1][:guest], args[1][:host]]
    end
    if @path.last == :trigger
      t = Trigger.new
      blk.call(t)
      $triggers[[$machine, args[0].to_s.split("::").last, name]] = t.code
      return nil
    end
    blk.call(*Array.new([blk.arity, 1].max) { Rec.new }) if blk
    Rec.new(@path + [name])
  end
end
class Trigger
  attr_reader :code
  def ruby(&blk); @code = blk; end
end
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); yield Rec.new; end
  def self.user_data_path; "/nonexistent"; end
end
load ARGV[0]
IsoloomHyperV.define_singleton_method(:ps) { |script| $ps << script; "" }
IsoloomHyperV.define_singleton_method(:address) { |*_| nil }
ui = Object.new
def ui.info(*_); end
machine = Struct.new(:name, :ssh_info, :ui).new(ARGV[1].to_sym, { host: "192.0.2.10" }, ui)
[["WaitForIPAddress", :after], ["DeleteVM", :after]].each do |action, at|
  code = $triggers[[ARGV[1], action, at]] or abort("no #{at} #{action} trigger for #{ARGV[1]}")
  code.call(nil, machine)
end
$forwards.each { |m, guest, host| puts "forward #{guest} #{host}" if m == ARGV[1] }
$ps.each { |s| puts "ps #{s}" }
"#;

/// `example`'s rendered outputs written under `dir`, as a copy of the project has them, with the
/// example's own folder as the project's root (`ISOLOOM_TEST_ROOT`), and Vagrant's
/// `vagrant/util/is_port_open` (which the Vagrantfile requires on a Mac) empty for `ruby -I dir`:
/// the Vagrantfile's path, and `ruby` ready to load it.
fn loadable(example: &str, dir: &Path) -> Outcome<(std::path::PathBuf, Command)> {
    let mut vagrantfile = None;
    for f in render(example)? {
        let path = dir.join(&f.path);
        std::fs::create_dir_all(path.parent().ok_or("a file at the root")?)?;
        // The project's root: the example's own folder (the outputs are written elsewhere).
        let contents = f
            .contents
            .replace("\nROOT = File.expand_path(\"../..\", __dir__)\n", "\nROOT = ENV.fetch(\"ISOLOOM_TEST_ROOT\")\n");
        std::fs::write(&path, contents)?;
        if f.path.ends_with("/Vagrantfile") {
            vagrantfile = Some(path);
        }
    }
    std::fs::create_dir_all(dir.join("vagrant/util"))?;
    std::fs::write(dir.join("vagrant/util/is_port_open.rb"), "")?;
    let mut ruby = Command::new("ruby");
    ruby.arg("-I").arg(dir);
    ruby.env(
        "ISOLOOM_TEST_ROOT",
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example),
    );
    Ok((vagrantfile.ok_or("no Vagrantfile")?, ruby))
}

/// What `ruby` printed, or why it failed.
fn printed(mut ruby: Command) -> Outcome<String> {
    let out = ruby.output()?;
    if !out.status.success() {
        return Err(format!("ruby failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    // Ruby's standard output is in text mode on Windows: each line ends in "\r\n" there.
    Ok(String::from_utf8(out.stdout)?.replace("\r\n", "\n"))
}

/// Whether `ruby` runs here (CI has it); a test that needs it says it is skipped otherwise.
fn ruby_runs() -> bool {
    Command::new("ruby").arg("-v").output().is_ok_and(|o| o.status.success())
}

/// What [`HYPERV_TRIGGERS_RB`] prints for machine `machine` of `example`'s Vagrantfile, with
/// `ISOLOOM_HOSTPORTS` set to `hostports` (unset when `None`). The Vagrantfile loads from its
/// rendered outputs, the example's folder its project.
fn hyperv_triggers(example: &str, machine: &str, hostports: Option<&str>) -> Outcome<String> {
    let scratch = tempfile::Builder::new().prefix("isoloom-hyperv-ports-").tempdir()?;
    let (vagrantfile, mut ruby) = loadable(example, scratch.path())?;
    ruby.arg("-e").arg(HYPERV_TRIGGERS_RB).arg(vagrantfile).arg(machine);
    match hostports {
        Some(json) => ruby.env("ISOLOOM_HOSTPORTS", json),
        None => ruby.env_remove("ISOLOOM_HOSTPORTS"),
    };
    printed(ruby)
}

/// On Hyper-V the run's URL is the port the runner chose: Hyper-V ignores `forwarded_port`, so
/// Isoloom relays the port itself, and the relay must listen on the host port the forward names
/// (`ISOLOOM_HOSTPORTS`), not on the spec's preferred one, which the run never reports. Without
/// the runner's choice both take the preferred one. The Ruby itself runs here when `ruby` is on
/// the PATH (CI has it).
#[test]
fn the_hyperv_relay_listens_on_the_host_port_the_forward_takes() -> Outcome {
    if !ruby_runs() {
        eprintln!("skipped: needs ruby");
        return Ok(());
    }
    let relay = |port: u16| format!("listenaddress=127.0.0.1 listenport={port} connectaddress=192.0.2.10 connectport=80");
    let gone = |port: u16| format!("netsh interface portproxy delete v4tov4 listenaddress=127.0.0.1 listenport={port} | Out-Null; $global:LASTEXITCODE = 0");
    for (hostports, port) in [(Some(r#"{"web/web":53544}"#), 53544), (None, 8080)] {
        let out = hyperv_triggers("solo-web", "web", hostports)?;
        assert!(out.contains(&format!("forward 80 {port}\n")), "the forward: {out}");
        assert!(out.contains(&relay(port)), "the relay at start: {out}");
        assert!(out.contains(&gone(port)), "the relay gone with the VM: {out}");
        let other = if port == 8080 { 53544 } else { 8080 };
        assert!(!out.contains(&format!("listenport={other}")), "a relay on another port: {out}");
    }
    Ok(())
}

/// Loads a generated Vagrantfile (`ARGV[0]`) on a stand-in for Vagrant, its machines left out
/// (`QEMU_HUBS` is set before them), and prints the QEMU hub port it picked for each VM, a
/// `<network> <vm> <port>` line each. With `ARGV[1]` `running`, every network's hub is taken to
/// run (its process the one its pid file names): the ports its record (`IsoloomQemu.ports_file`)
/// holds are then its own.
const QEMU_PORTS_RB: &str = r##"
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); end
end
if ARGV[1] == "running"
  # Ahead of the file's own `running`, which it defines when it loads.
  module IsoloomQemu; end
  IsoloomQemu.singleton_class.prepend(Module.new { def running(_net); 4242; end })
end
load ARGV[0]
QEMU_HUBS.each { |net, vms| vms.each { |vm, port| puts "#{net} #{vm} #{port}" } }
"##;

/// The QEMU hub ports `example`'s Vagrantfile picks when loaded from the copy of the project at
/// `dir` (see [`QEMU_PORTS_RB`]), as `<network>/<vm>` and port, and the Vagrantfile's path.
fn qemu_hub_ports(example: &str, dir: &Path, running: bool) -> Outcome<(Vec<(String, u16)>, std::path::PathBuf)> {
    let (vagrantfile, mut ruby) = loadable(example, dir)?;
    ruby.arg("-e").arg(QEMU_PORTS_RB).arg(&vagrantfile);
    if running {
        ruby.arg("running");
    }
    let ports = printed(ruby)?
        .lines()
        .map(|l| match l.split(' ').collect::<Vec<_>>()[..] {
            [net, vm, port] => Ok((format!("{net}/{vm}"), port.parse()?)),
            _ => Err(format!("not a hub port: {l}").into()),
        })
        .collect::<Outcome<_>>()?;
    Ok((ports, vagrantfile))
}

/// Two copies of one environment (its name, two folders) run side by side under QEMU: their
/// networks' hubs listen on the host's loopback, so their ports are the copy's, never the spec's.
/// They were one block from a hash of the environment's name, and a second `solo-web` failed to
/// start its hub (the first held 39761 and 39762). Each copy's ports are distinct, in
/// 20000-39999, and kept from one read of its Vagrantfile to the next.
#[test]
fn two_copies_of_one_environment_get_disjoint_qemu_hub_ports() -> Outcome {
    if !ruby_runs() {
        eprintln!("skipped: needs ruby");
        return Ok(());
    }
    let scratch = tempfile::Builder::new().prefix("isoloom-qemu-hubs-").tempdir()?;
    for example in ["solo-web", "segmented"] {
        let mut copies = Vec::new();
        for copy in ["one", "two"] {
            let dir = scratch.path().join(example).join(copy);
            let (ports, _) = qemu_hub_ports(example, &dir, false)?;
            assert!(ports.len() >= 2, "{example}: {ports:?}");
            assert!(ports.iter().all(|(_, p)| (20000..40000).contains(p)), "{example}: {ports:?}");
            let distinct: std::collections::BTreeSet<u16> = ports.iter().map(|(_, p)| *p).collect();
            assert_eq!(distinct.len(), ports.len(), "{example} {copy}: two VMs share a port: {ports:?}");
            assert_eq!(qemu_hub_ports(example, &dir, false)?.0, ports, "{example} {copy}: read again");
            copies.push(distinct);
        }
        assert!(copies[0].is_disjoint(&copies[1]), "{example}: the copies share ports: {copies:?}");
    }
    Ok(())
}

/// A copy's hub that runs keeps its ports, even taken (it holds them); one that doesn't run gets a
/// port that is free in place of one that something else holds.
#[test]
fn a_running_hub_keeps_its_ports_and_a_held_port_is_passed_over() -> Outcome {
    if !ruby_runs() {
        eprintln!("skipped: needs ruby");
        return Ok(());
    }
    let scratch = tempfile::Builder::new().prefix("isoloom-qemu-held-").tempdir()?;
    let (ports, vagrantfile) = qemu_hub_ports("solo-web", scratch.path(), false)?;
    let held = std::net::TcpListener::bind(("127.0.0.1", ports[0].1))?;
    // What the hub's start recorded: its ports, by VM.
    let hubs = vagrantfile.parent().ok_or("a folder")?.join(".vagrant/isoloom-qemu-hubs");
    std::fs::create_dir_all(&hubs)?;
    let record: Vec<String> = ports
        .iter()
        .map(|(key, p)| format!("\"{}\":{p}", key.split_once('/').map_or(key.as_str(), |(_, vm)| vm)))
        .collect();
    std::fs::write(hubs.join("app.ports.json"), format!("{{{}}}", record.join(",")))?;

    assert_eq!(qemu_hub_ports("solo-web", scratch.path(), true)?.0, ports, "the running hub's own");
    let (fresh, _) = qemu_hub_ports("solo-web", scratch.path(), false)?;
    assert_ne!(fresh[0].1, ports[0].1, "a held port: {fresh:?}");
    assert_eq!(fresh.len(), ports.len());
    drop(held);
    Ok(())
}

/// Loads a generated Vagrantfile (`ARGV[0]`) as [`QEMU_PORTS_RB`] does, then starts network
/// `ARGV[1]`'s hub as the trigger before a VM's start does, on a stand-in for QEMU: it prints each
/// VM's port (`port <vm> <port>`), then what happened to the hub (`stop`, `start <ports>`), then
/// its record (`record <json>`) or the refusal (`refused <why>`). `HUB=running` takes the hub to
/// run, on the record the test wrote.
const QEMU_HUB_RB: &str = r#"
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); end
end
module IsoloomQemu; end
IsoloomQemu.singleton_class.prepend(Module.new {
  def running(_net); ENV["HUB"] == "running" ? 4242 : nil; end
  def stop(net); puts "stop #{net}"; end
  def system(*args, **_opts)
    puts "start #{args.map { |a| a[/listen=127\.0\.0\.1:(\d+)/, 1] }.compact.join(' ')}"
    true
  end
})
load ARGV[0]
net = ARGV[1]
QEMU_HUBS[net].each { |vm, port| puts "port #{vm} #{port}" }
begin
  IsoloomQemu.hub(net, QEMU_HUBS[net])
  puts "record #{File.read(IsoloomQemu.ports_file(net))}"
rescue Vagrant::Errors::VagrantError => e
  puts "refused #{e.error_message}"
end
"#;

/// What [`QEMU_HUB_RB`] prints for `example`'s network `net`, from the copy at `dir`, the hub
/// running when `running`: its lines, and the hub's record file.
fn qemu_hub(example: &str, dir: &Path, net: &str, running: bool) -> Outcome<(String, std::path::PathBuf)> {
    let (vagrantfile, mut ruby) = loadable(example, dir)?;
    ruby.arg("-e").arg(QEMU_HUB_RB).arg(&vagrantfile).arg(net);
    if running {
        ruby.env("HUB", "running");
    } else {
        ruby.env_remove("HUB");
    }
    let record = vagrantfile
        .parent()
        .ok_or("a folder")?
        .join(format!(".vagrant/isoloom-qemu-hubs/{net}.ports.json"));
    Ok((printed(ruby)?, record))
}

/// The `port <vm> <port>` lines of [`QEMU_HUB_RB`]'s output, in order.
fn vm_ports(out: &str) -> Vec<(String, u16)> {
    out.lines()
        .filter_map(|l| l.strip_prefix("port "))
        .filter_map(|l| l.split_once(' ').and_then(|(vm, p)| Some((vm.to_owned(), p.parse().ok()?))))
        .collect()
}

/// A VM that joins a running environment's network on QEMU (the attack box the Cyber CTF launcher
/// adds) must leave the VMs already there on it. 2.1.1 kept a running hub only for exactly the VMs
/// it started with: one more gave every VM new ports and restarted the hub, and a running VM,
/// whose socket netdev doesn't reconnect, was off the network for good ("No route to host" from
/// the attack box to `web`). A hub starts with spare ports; a VM that joins takes one, the others
/// keep theirs, and the hub runs on.
#[test]
fn a_vm_joining_a_running_qemu_network_takes_a_spare_port_and_the_hub_runs_on() -> Outcome {
    if !ruby_runs() {
        eprintln!("skipped: needs ruby");
        return Ok(());
    }
    let scratch = tempfile::Builder::new().prefix("isoloom-qemu-join-").tempdir()?;
    // A first start: the VMs' ports and spare ones beyond them, all listened on, all recorded.
    let (out, record) = qemu_hub("solo-web", scratch.path(), "app", false)?;
    let ports = vm_ports(&out);
    assert!(ports.len() >= 2, "{out}");
    let started: Vec<u16> = out
        .lines()
        .find_map(|l| l.strip_prefix("start "))
        .ok_or("no start")?
        .split(' ')
        .map(str::parse::<u16>)
        .collect::<Result<_, _>>()?;
    assert_eq!(started.len(), ports.len() + 4, "the VMs' ports and 4 spare ones: {out}");
    assert_eq!(&started[..ports.len()], ports.iter().map(|(_, p)| *p).collect::<Vec<_>>(), "{out}");
    let spare = started[ports.len()..].to_vec();
    let distinct: std::collections::BTreeSet<u16> = started.iter().copied().collect();
    assert_eq!(distinct.len(), started.len(), "a port twice: {out}");

    // The last VM joins once the others run: the hub's record holds the others and the spares.
    let (joined, others) = ports.split_last().ok_or("a VM")?;
    let vms: Vec<String> = others.iter().map(|(vm, p)| format!("\"{vm}\":{p}")).collect();
    let spares: Vec<String> = spare.iter().map(ToString::to_string).collect();
    std::fs::write(&record, format!("{{\"vms\":{{{}}},\"spare\":[{}]}}", vms.join(","), spares.join(",")))?;
    let (joining, _) = qemu_hub("solo-web", scratch.path(), "app", true)?;
    let now = vm_ports(&joining);
    assert_eq!(&now[..others.len()], others, "the running VMs keep their ports: {joining}");
    assert_eq!(
        now[others.len()],
        (joined.0.clone(), spare[0]),
        "the joining VM takes the first spare port: {joining}"
    );
    assert!(
        !joining.contains("\nstop ") && !joining.contains("\nstart "),
        "the hub was restarted under the running VMs: {joining}"
    );
    let kept = std::fs::read_to_string(&record)?;
    assert!(kept.contains(&format!("\"{}\":{}", joined.0, spare[0])), "{kept}");
    assert!(kept.contains(&format!("\"spare\":[{}]", spares[1..].join(","))), "{kept}");
    Ok(())
}

/// A hub with no spare port left for a VM that joins (one an earlier Isoloom started, whose record
/// is the ports by VM alone) is not restarted under its VMs: the start is refused, with the way
/// out.
#[test]
fn a_running_qemu_hub_without_a_spare_port_refuses_a_joining_vm_rather_than_restart() -> Outcome {
    if !ruby_runs() {
        eprintln!("skipped: needs ruby");
        return Ok(());
    }
    let scratch = tempfile::Builder::new().prefix("isoloom-qemu-full-").tempdir()?;
    let (out, record) = qemu_hub("solo-web", scratch.path(), "app", false)?;
    let ports = vm_ports(&out);
    let (_, others) = ports.split_last().ok_or("a VM")?;
    let vms: Vec<String> = others.iter().map(|(vm, p)| format!("\"{vm}\":{p}")).collect();
    std::fs::write(&record, format!("{{{}}}", vms.join(",")))?;
    let (joining, _) = qemu_hub("solo-web", scratch.path(), "app", true)?;
    assert!(
        !joining.contains("\nstop ") && !joining.contains("\nstart "),
        "the hub was restarted under the running VMs: {joining}"
    );
    let why = joining
        .lines()
        .find_map(|l| l.strip_prefix("refused "))
        .ok_or_else(|| format!("not refused: {joining}"))?;
    assert!(why.contains("no spare port left") && why.contains("Destroy the environment"), "{why}");
    Ok(())
}

/// The run manifest names each network as the Vagrantfile makes it: VirtualBox's internal
/// network and libvirt's network, both `isoloom-<environment>-<network>`.
#[test]
fn the_run_manifest_names_each_network_as_the_vagrantfile_makes_it() -> Outcome {
    let env = plan("segmented")?;
    let manifest = VagrantBackend.run_manifest(&env)?.ok_or("a run manifest")?;
    let file = vagrantfile("segmented")?;
    assert_eq!(manifest.networks.len(), env.networks.len());
    for n in &manifest.networks {
        let native = n.native.as_ref().map(ToString::to_string).ok_or("a native name")?;
        assert!(native.starts_with(isoloom_manifest::contract::RESERVED_PREFIX), "{native}");
        assert!(file.contains(&format!("virtualbox__intnet: \"{native}\"")), "{native}");
        assert!(file.contains(&format!("libvirt__network_name: \"{native}\"")), "{native}");
    }
    Ok(())
}
