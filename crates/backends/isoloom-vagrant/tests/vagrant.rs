//! The Vagrant backend's rendering of a few examples, as snapshots, its determinism, and its
//! refusals from the plan.

#![expect(
    clippy::disallowed_methods,
    reason = "the tests run the binary and the tools that check its output: test code, not an adapter"
)]

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
    Ok(lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?)
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
    Ok(lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None)?)
}

/// Fails naming the first of `tools` that doesn't run here: the test is ignored by default (a
/// player's machine lacks it) and `cargo xtask test` runs it, so a missing tool says so.
fn needs(tools: &[&str]) -> Outcome {
    for tool in tools {
        if !Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success()) {
            return Err(format!("needs `{tool}` on PATH").into());
        }
    }
    Ok(())
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
    let env = lab.plan(&spec, Target::Vagrant, None)?;
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
    let host = "ENV.fetch(\"ISOTHRUM_HOST_PORT_WEB_80_TCP\", ISOLOOM_HOSTPORTS.fetch(\"web/web\", 8080)).to_i";
    assert!(file.contains(&format!("forwarded_port\", guest: 80, host: {host},")), "the forward");
    assert!(file.contains(&format!("IsoloomHyperV.ports(machine, [[80, {host}]])")), "the relay at start");
    assert!(file.contains(&format!("IsoloomHyperV.unports([[80, {host}]])")), "the relay gone with the VM");
    let controller = file.split("config.vm.define \"isoloom-controller\"").nth(1).ok_or("no controller")?;
    let hyperv = controller.split("provider \"hyperv\" do |v, o|").nth(1).ok_or("no hyperv block")?;
    assert!(hyperv.split("\n    end").next().unwrap_or_default().contains("o.vm.hostname = nil"));
    Ok(())
}

/// A stand-in for Vagrant that records what a generated Vagrantfile (`ARGV[0]`) says, then loads
/// it: each VM's forwarded ports (`$forwards`) and typed action triggers (`$triggers`, by VM, the
/// action's name and when). Vagrant's own Ruby is 3.x; macOS's `ruby` is 2.6, which lacks
/// `filter_map`, so it gets one.
const VAGRANT_STAND_IN_RB: &str = r#"
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
"#;

/// After [`VAGRANT_STAND_IN_RB`]: fires machine `ARGV[1]`'s Hyper-V triggers after
/// `WaitForIPAddress` and `DeleteVM` on a VM at 192.0.2.10, Hyper-V's PowerShell recorded instead
/// of run. Prints each forwarded port (`forward <guest> <host>`) and each PowerShell script
/// (`ps <script>`).
const HYPERV_TRIGGERS_RB: &str = r#"
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

/// What [`HYPERV_TRIGGERS_RB`] prints for machine `machine` of `example`'s Vagrantfile, with
/// `ISOLOOM_HOSTPORTS` set to `hostports` (unset when `None`). The Vagrantfile loads from its
/// rendered outputs, the example's folder its project.
fn hyperv_triggers(example: &str, machine: &str, hostports: Option<&str>) -> Outcome<String> {
    let scratch = tempfile::Builder::new().prefix("isoloom-hyperv-ports-").tempdir()?;
    let (vagrantfile, mut ruby) = loadable(example, scratch.path())?;
    ruby.arg("-e")
        .arg([VAGRANT_STAND_IN_RB, HYPERV_TRIGGERS_RB].concat())
        .arg(vagrantfile)
        .arg(machine);
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
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn the_hyperv_relay_listens_on_the_host_port_the_forward_takes() -> Outcome {
    needs(&["ruby"])?;
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

/// A test's scratch folder that, dropped, stops every QEMU hub a Vagrantfile under it left
/// running, found by its pid file: a hub runs as a daemon (`-daemonize`), so one a test leaves
/// outlives it, holding every file it inherited open. [`HubScratch::no_hub_left`] fails the test
/// that left one.
struct HubScratch(tempfile::TempDir);

impl HubScratch {
    fn new(prefix: &str) -> Outcome<HubScratch> {
        Ok(HubScratch(tempfile::Builder::new().prefix(prefix).tempdir()?))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    /// Stops the hubs still running under the folder: each pid file's process, when it is the
    /// hub (its command names the pid file, as `IsoloomQemu.running` checks), not another process
    /// given the id since. Those it found running, by pid file, each said stopped or not.
    fn stop_hubs(&self) -> Vec<String> {
        let mut stopped = Vec::new();
        for pid_file in hub_pid_files(self.path()) {
            let Some(pid) = std::fs::read_to_string(&pid_file).ok().and_then(|s| s.trim().parse::<u32>().ok()) else {
                continue;
            };
            let command = Command::new("ps").args(["-ww", "-o", "command=", "-p", &pid.to_string()]).output();
            if command.is_ok_and(|c| String::from_utf8_lossy(&c.stdout).contains(&*pid_file.to_string_lossy())) {
                // A stand-in or a real hub alike: neither needs a clean stop here.
                let killed = Command::new("kill").args(["-KILL", &pid.to_string()]).status();
                let how = if killed.is_ok_and(|k| k.success()) { "stopped" } else { "not stopped" };
                stopped.push(format!("{} (pid {pid}, {how})", pid_file.display()));
            }
        }
        stopped
    }

    /// Fails when a hub still runs under the folder (stopping it): a test stops what it starts.
    fn no_hub_left(&self) -> Outcome {
        let left = self.stop_hubs();
        if left.is_empty() {
            Ok(())
        } else {
            Err(format!("QEMU hubs outlived the test: {left:?}").into())
        }
    }
}

impl Drop for HubScratch {
    fn drop(&mut self) {
        self.stop_hubs();
    }
}

/// The hubs' pid files under `dir`: `.vagrant/isoloom-qemu-hubs/<network>.pid` beside each
/// Vagrantfile.
fn hub_pid_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            found.extend(hub_pid_files(&path));
        } else if path.extension().is_some_and(|e| e == "pid") && path.parent().and_then(Path::file_name).is_some_and(|n| n == "isoloom-qemu-hubs") {
            found.push(path);
        }
    }
    found
}

/// A hub a test left running is stopped by its scratch folder, and the test that left it fails.
#[cfg(unix)]
#[test]
fn a_hub_left_running_is_stopped_and_fails_its_test() -> Outcome {
    let scratch = HubScratch::new("isoloom-qemu-left-")?;
    let hubs = scratch.path().join("project/.vagrant/isoloom-qemu-hubs");
    std::fs::create_dir_all(&hubs)?;
    let pid_file = hubs.join("app.pid");
    // A stand-in hub, as the Ruby tests start one: a daemon whose command names its pid file.
    let mut hub = Command::new("sh").arg("-c").arg("while :; do sleep 0.1; done").arg(&pid_file).spawn()?;
    std::fs::write(&pid_file, format!("{}\n", hub.id()))?;
    assert!(scratch.no_hub_left().is_err(), "a running hub is a failure");
    let status = hub.wait()?;
    assert!(!status.success(), "the hub was stopped: {status}");
    scratch.no_hub_left()
}

/// Two copies of one environment (its name, two folders) run side by side under QEMU: their
/// networks' hubs listen on the host's loopback, so their ports are the copy's, never the spec's.
/// They were one block from a hash of the environment's name, and a second `solo-web` failed to
/// start its hub (the first held 39761 and 39762). Each copy's ports are distinct, in
/// 20000-39999, and kept from one read of its Vagrantfile to the next.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn two_copies_of_one_environment_get_disjoint_qemu_hub_ports() -> Outcome {
    needs(&["ruby"])?;
    let scratch = HubScratch::new("isoloom-qemu-hubs-")?;
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
    scratch.no_hub_left()
}

/// A copy's hub that runs keeps its ports, even taken (it holds them); one that doesn't run gets a
/// port that is free in place of one that something else holds.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn a_running_hub_keeps_its_ports_and_a_held_port_is_passed_over() -> Outcome {
    needs(&["ruby"])?;
    let scratch = HubScratch::new("isoloom-qemu-held-")?;
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
    scratch.no_hub_left()
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

/// A VM that joins a running environment's network on QEMU (an access machine an embedder adds)
/// must leave the VMs already there on it. A hub sized for exactly the VMs it started with would
/// give every VM new ports and restart for one more, and a running VM, whose socket netdev
/// doesn't reconnect, would be off the network for good ("No route to host" to `web`). A hub
/// starts with spare ports; a VM that joins takes one, the others keep theirs, and the hub runs
/// on.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn a_vm_joining_a_running_qemu_network_takes_a_spare_port_and_the_hub_runs_on() -> Outcome {
    needs(&["ruby"])?;
    let scratch = HubScratch::new("isoloom-qemu-join-")?;
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
    scratch.no_hub_left()
}

/// A hub with no spare port left for a VM that joins (one an earlier Isoloom started, whose record
/// is the ports by VM alone) is not restarted under its VMs: the start is refused, with the way
/// out.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn a_running_qemu_hub_without_a_spare_port_refuses_a_joining_vm_rather_than_restart() -> Outcome {
    needs(&["ruby"])?;
    let scratch = HubScratch::new("isoloom-qemu-full-")?;
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
    scratch.no_hub_left()
}

/// Loads a generated Vagrantfile (`ARGV[0]`) on a stand-in for Vagrant and plays a VM's visit to
/// network `app` on QEMU, Vagrant's machine folders under `ARGV[1]`:
/// 1. The environment's VMs start, and their hub with them.
/// 2. A VM joins the running network (an access machine an embedder adds), then is
///    destroyed again.
/// 3. The environment is destroyed.
///
/// QEMU's hub is stood in for by a process that names its pid file (as `running` checks for),
/// writes its pid there (`-daemonize -pidfile`), and removes it as it exits on TERM, as QEMU does.
/// Vagrant destroys QEMU's VMs in parallel: as the last VM's trigger stops the hub, the other VM's
/// destroy ends it, pid file and all, between the stop's check and its signal. It prints what it
/// sees after each step.
#[cfg(unix)]
const QEMU_HUB_VISIT_RB: &str = r##"
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); end
end
module IsoloomQemu; end
$hub = nil
$hubs = []
$racing = false
$stopping = false
at_exit { $hubs.each { |pid| Process.kill("KILL", pid) rescue nil } }
IsoloomQemu.singleton_class.prepend(Module.new {
  def system(*args, **_opts)
    pidfile = args[args.index("-pidfile") + 1]
    $hub = Process.spawn("sh", "-c", 'trap "rm -f \"$0\"; exit 0" TERM; while :; do sleep 0.1; done', pidfile)
    $hubs << $hub
    File.write(pidfile, "#{$hub}\n")
    true
  end
  def stop(net)
    $stopping = true
    super
  ensure
    $stopping = false
  end
  def running(net)
    pid = super
    if pid && $racing && $stopping
      # The other VM's destroy stops the hub now: it removes its pid file and exits.
      $racing = false
      Process.kill("TERM", pid)
      Process.wait(pid)
    end
    pid
  end
})
load ARGV[0]
require "fileutils"
require "pathname"
net = "app"
data = Pathname.new(ARGV[1])
env = Struct.new(:local_data_path).new(data)
up = ->(vm) { FileUtils.mkdir_p(data.join("machines", vm, "qemu")); File.write(data.join("machines", vm, "qemu", "id"), vm) }
destroyed = ->(vm) { FileUtils.rm_rf(data.join("machines", vm)) }
vms = QEMU_HUBS[net]
vms.each_key(&up)
IsoloomQemu.hub(net, vms)
hub = $hub
puts "up #{IsoloomQemu.running(net) == hub}"
with_box = IsoloomQemu.ports(net => vms.keys + ["attackbox"])[net]
IsoloomQemu.hub(net, with_box)
up.("attackbox")
puts "joined #{$hub == hub}"
destroyed.("attackbox")
IsoloomQemu.release(net, with_box.keys, env, "attackbox")
puts "left #{IsoloomQemu.running(net) == hub}"
vms.each_key(&destroyed)
$racing = true
vms.each_key.reverse_each { |vm| IsoloomQemu.release(net, vms.keys, env, vm) }
puts "destroyed #{IsoloomQemu.running(net).inspect} #{Dir.children(IsoloomQemu.hubs_dir).sort.inspect}"
"##;

/// An environment on QEMU is destroyed after a VM joined its running network and left it again
/// (an access machine an embedder adds and removes). The hub's stop checks the hub and its files,
/// then signals it and deletes them, and the hub (QEMU removes its pid file as it exits, and
/// Vagrant destroys QEMU's VMs in parallel) can be gone in between, which must not fail the
/// destroy ("No such file or directory @ apply2files - …/isoloom-qemu-hubs/app.pid
/// (`Errno::ENOENT`)"): a hub or a file already gone is no error, and the hub runs on until the
/// last VM is destroyed.
#[cfg(unix)]
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn an_environment_on_qemu_is_destroyed_after_a_vm_joined_and_left_its_network() -> Outcome {
    needs(&["ruby"])?;
    let scratch = HubScratch::new("isoloom-qemu-visit-")?;
    let (vagrantfile, mut ruby) = loadable("solo-web", &scratch.path().join("project"))?;
    ruby.arg("-e").arg(QEMU_HUB_VISIT_RB).arg(&vagrantfile).arg(scratch.path().join("machines"));
    let out = printed(ruby)?;
    let said: Vec<&str> = out.lines().collect();
    assert_eq!(
        said,
        ["up true", "joined true", "left true", "destroyed nil [\"app.lock\"]"],
        "the hub runs on through the visit and goes, files and all (its lock kept), with the environment"
    );
    scratch.no_hub_left()
}

/// Loads a generated Vagrantfile (`ARGV[0]`) on a stand-in for Vagrant and starts network `app`'s
/// VMs on QEMU at once, as Vagrant does (`parallel: true`): each VM's trigger asks for the hub on
/// a thread of its own. QEMU's hub is stood in for by a start that takes a while (its ports to
/// listen on) before it writes its pid file, and a stop that ends whatever hub runs. It prints how
/// many hubs were started and stopped, and whether one runs.
#[cfg(unix)]
const QEMU_HUB_RACE_RB: &str = r#"
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); end
end
module IsoloomQemu; end
$started = 0
$stopped = 0
IsoloomQemu.singleton_class.prepend(Module.new {
  def running(net); File.exist?(pid_file(net)) ? 4242 : nil; end
  def stop(net)
    $stopped += 1 if running(net)
    File.delete(pid_file(net)) if running(net)
  end
  def system(*args, **_opts)
    sleep 0.3
    $started += 1
    File.write(args[args.index("-pidfile") + 1], "4242\n")
    true
  end
})
load ARGV[0]
net = "app"
vms = QEMU_HUBS[net]
vms.keys.map { Thread.new { IsoloomQemu.hub(net, vms) } }.each(&:join)
puts "started #{$started} stopped #{$stopped} running #{!IsoloomQemu.running(net).nil?}"
"#;

/// Regression: two VMs of one network started at once (Vagrant starts QEMU's VMs in parallel) each
/// found no hub and started one, the second stopping the first's hub under the VM that had just
/// connected to it. A network's hub is decided under its lock: one start, the other VM joining it.
#[cfg(unix)]
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn vms_starting_at_once_on_qemu_share_one_hub() -> Outcome {
    needs(&["ruby"])?;
    let scratch = tempfile::Builder::new().prefix("isoloom-qemu-race-").tempdir()?;
    let (vagrantfile, mut ruby) = loadable("solo-web", scratch.path())?;
    ruby.arg("-e").arg(QEMU_HUB_RACE_RB).arg(&vagrantfile);
    let out = printed(ruby)?;
    assert_eq!(out.trim(), "started 1 stopped 0 running true", "one hub for the VMs starting at once");
    Ok(())
}

/// Loads a generated Vagrantfile (`ARGV[0]`) on a stand-in for Vagrant and prints this user's uid
/// and gid, then the arguments QEMU's share hands virtiofsd (`IsoloomQemu.virtiofs_squash`), one a
/// line.
#[cfg(unix)]
const VIRTIOFS_RB: &str = r##"
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); end
end
load ARGV[0]
puts "#{Process.uid} #{Process.gid}"
IsoloomQemu.virtiofs_squash.each { |a| puts a }
"##;

/// What [`VIRTIOFS_RB`] prints with `virtiofsd` on the PATH answering `--help` with `help`.
#[cfg(unix)]
fn virtiofs_args(help: &str) -> Outcome<Vec<String>> {
    let scratch = tempfile::Builder::new().prefix("isoloom-virtiofs-").tempdir()?;
    let yaml = "version: 1\nname: shared-desk\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nmachines:\n  desk: { os: debian@12, networks: { lan: 10 }, host_folders: [{ from: shared, path: /shared }] }\n";
    let mut vagrantfile = None;
    for f in VagrantBackend.render(&parsed(yaml)?)? {
        let path = scratch.path().join(&f.path);
        std::fs::create_dir_all(path.parent().ok_or("a file at the root")?)?;
        std::fs::write(&path, &f.contents)?;
        if f.path.ends_with("/Vagrantfile") {
            vagrantfile = Some(path);
        }
    }
    std::fs::create_dir_all(scratch.path().join("vagrant/util"))?;
    std::fs::write(scratch.path().join("vagrant/util/is_port_open.rb"), "")?;
    let bin = scratch.path().join("bin");
    std::fs::create_dir_all(&bin)?;
    let virtiofsd = bin.join("virtiofsd");
    // Written by a child process: one this test wrote itself could be busy ("Text file busy") in
    // a process another test forked meanwhile, when Ruby runs it.
    isoloom_test_support::write_executable(&virtiofsd, &format!("#!/bin/sh\nprintf '%s\\n' '{help}'\n"), true)?;
    let mut ruby = Command::new("ruby");
    ruby.arg("-I")
        .arg(scratch.path())
        .arg("-e")
        .arg(VIRTIOFS_RB)
        .arg(vagrantfile.ok_or("no Vagrantfile")?);
    ruby.env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    Ok(printed(ruby)?.lines().map(str::to_owned).collect())
}

/// A guest account other than the SSH user (uid 1001 in group 1000 of a `drwxrwxr-x 1000:1000`
/// share) gets EPERM writing the host folder when vagrant-qemu's virtiofsd maps uid and gid 1000
/// alone. With a virtiofsd that translates ids, every other guest uid and gid is squashed to this
/// computer's user and group, in two ranges either side of 1000 that never overlap vagrant-qemu's
/// map; with one that doesn't, no argument is added, so it still starts.
#[cfg(unix)]
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn qemus_virtiofsd_squashes_every_other_guest_account_where_it_translates_ids() -> Outcome {
    needs(&["ruby"])?;
    let translating = virtiofs_args("    --translate-uid <type>:<source base>:<target base>:<count>")?;
    let (ids, args) = translating.split_first().ok_or("no output")?;
    let (uid, gid) = ids.split_once(' ').ok_or("no ids")?;
    let squash = |flag: &str, host: &str| {
        [
            flag.to_owned(),
            format!("squash-guest:0:{host}:1000"),
            flag.to_owned(),
            format!("squash-guest:1001:{host}:4294966294"),
        ]
    };
    let expected: Vec<String> = squash("--translate-uid", uid).into_iter().chain(squash("--translate-gid", gid)).collect();
    assert_eq!(args, expected);
    let older = virtiofs_args("Usage: virtiofsd [OPTIONS] --shared-dir <shared_dir>")?;
    assert_eq!(older.len(), 1, "a virtiofsd translating no ids gets no argument: {older:?}");
    Ok(())
}

/// The Vagrantfile names each network after the environment, its key and the network, VirtualBox's
/// internal network and libvirt's alike (`isoloom-<environment>-#{ISOLOOM_KEY}-<network>`).
#[test]
fn each_network_is_named_with_the_copy_key() -> Outcome {
    let env = plan("segmented")?;
    let networks = libvirt_networks(&vagrantfile("segmented")?);
    let keyed = |name: &str| format!("{}segmented-#{{ISOLOOM_KEY}}-{name}", isoloom_spec::contract::RESERVED_PREFIX);
    // Every VM's every private network is named so, by both providers.
    for (vm, kw) in &networks {
        assert_eq!(field(kw, "virtualbox__intnet"), field(kw, "libvirt__network_name"), "{vm}: {kw:?}");
    }
    let named: std::collections::BTreeSet<&str> = networks.iter().map(|(_, kw)| field(kw, "libvirt__network_name")).collect();
    let wanted: std::collections::BTreeSet<String> = env.networks.keys().map(|n| keyed(n)).collect();
    assert_eq!(named, wanted.iter().map(String::as_str).collect());
    Ok(())
}

/// Loads a generated Vagrantfile (`ARGV[0]`) on a stand-in for Vagrant that records every name it
/// gives on the host, one `<what> <name>` line each: a VM's (`name=`, `vmname=`, `guest_name=`,
/// VMware's `displayName`), libvirt's domain prefix, and each private network's internal network
/// and libvirt network.
const HOST_NAMES_RB: &str = r##"
Enumerable.module_eval { def filter_map; each_with_object([]) { |x, out| v = yield(x); out << v if v }; end } unless Enumerable.method_defined?(:filter_map)
NAMED = %i[name= vmname= guest_name= default_prefix=].freeze
class Rec
  def initialize(path = []); @path = path; end
  def respond_to_missing?(*); true; end
  def method_missing(name, *args, &blk)
    puts "#{name.to_s.chomp("=")} #{args[0]}" if NAMED.include?(name) && args[0].is_a?(String)
    puts "displayName #{args[1]}" if name == :[]= && args[0] == "displayName"
    if name == :network && args[1].is_a?(Hash)
      %i[virtualbox__intnet libvirt__network_name].each { |k| puts "#{k} #{args[1][k]}" if args[1][k] }
    end
    # A trigger's code runs when Vagrant acts, not when it reads the file.
    return Rec.new if @path.last == :trigger
    blk.call(*Array.new([blk.arity, 1].max) { Rec.new }) if blk
    Rec.new(@path + [name])
  end
end
module Vagrant
  module Util; end
  module Errors; class VagrantError < StandardError; end; end
  def self.configure(_version); yield Rec.new; end
  def self.user_data_path; "/nonexistent"; end
  def self.has_plugin?(*); false; end
end
load ARGV[0]
"##;

/// The names `example`'s Vagrantfile gives on the host when loaded from the copy at `dir`, with
/// `ISOLOOM_KEY` set to `key` (unset: the Vagrantfile makes one from its folder).
fn vagrant_host_names(example: &str, dir: &Path, key: Option<&str>) -> Outcome<std::collections::BTreeSet<String>> {
    let (vagrantfile, mut ruby) = loadable(example, dir)?;
    ruby.arg("-e").arg(HOST_NAMES_RB).arg(&vagrantfile);
    match key {
        Some(key) => ruby.env("ISOLOOM_KEY", key),
        None => ruby.env_remove("ISOLOOM_KEY"),
    };
    Ok(printed(ruby)?.lines().map(str::to_owned).collect())
}

/// Two copies of one environment (one spec, two folders, no instance number) never share a VM,
/// network or libvirt prefix name on any provider: each name carries the copy's key, the runner's
/// (`ISOLOOM_KEY`) or, run by hand, one made from the copy's folder, the same at every read.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn two_copies_of_one_environment_never_share_a_name_on_the_host() -> Outcome {
    needs(&["ruby"])?;
    let scratch = tempfile::Builder::new().prefix("isoloom-copy-names-").tempdir()?;
    for example in ["solo-web", "segmented"] {
        let (one, two) = (scratch.path().join(example).join("one"), scratch.path().join(example).join("two"));
        let by_hand_one = vagrant_host_names(example, &one, None)?;
        let by_hand_two = vagrant_host_names(example, &two, None)?;
        assert!(by_hand_one.len() >= 4, "{example}: {by_hand_one:?}");
        let shared: Vec<_> = by_hand_one.intersection(&by_hand_two).collect();
        assert!(shared.is_empty(), "{example}: {shared:?}");
        assert_eq!(vagrant_host_names(example, &one, None)?, by_hand_one, "{example}: read again");
        let runs_a = vagrant_host_names(example, &one, Some("aaaaa1"))?;
        let runs_b = vagrant_host_names(example, &one, Some("bbbbb2"))?;
        assert!(runs_a.is_disjoint(&runs_b), "{example}: two runs' keys");
        assert!(!runs_a.is_empty() && runs_a.iter().all(|l| l.contains("-aaaaa1")), "{example}: {runs_a:?}");
        assert!(runs_a.contains(&format!("default_prefix {example}-aaaaa1_")), "{example}: {runs_a:?}");
        let label = format!("name {example}-aaaaa1 - ");
        assert!(runs_a.iter().any(|l| l.starts_with(&label)), "{example}: {runs_a:?}");
    }
    Ok(())
}

/// Every example Vagrant renders, with its Vagrantfile.
fn every_vagrantfile() -> Outcome<Vec<(String, String)>> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(examples)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        // An example Vagrant refuses (an appliance, a container only) has no Vagrantfile.
        if let Ok(file) = vagrantfile(&name) {
            out.push((name, file));
        }
    }
    out.sort();
    Ok(out)
}

/// Each `config.vm.define` of `file`: its name, and each of its provisioners as its name and its
/// text (the `vm.provision` line up to the next one, or the define's end).
fn defines(file: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in file.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("config.vm.define \"") {
            out.push((rest.split('"').next().unwrap_or_default().to_owned(), Vec::new()));
        } else if let Some((_, provisioners)) = out.last_mut() {
            if line.contains(".vm.provision ") {
                let name = line
                    .split("name: \"")
                    .nth(1)
                    .or_else(|| line.split(".vm.provision \"").nth(1))
                    .unwrap_or_default();
                provisioners.push((name.split('"').next().unwrap_or_default().to_owned(), String::new()));
            }
            if let Some((_, text)) = provisioners.last_mut() {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    out
}

/// The provisioners that converge, run again by `vagrant provision`: each writes the same state
/// again (`>`, `replace`, a line added once, a unit rewritten), waits, checks, or is skipped by
/// its own mark (`setup`). A provisioner named otherwise (a project script, a shared volume) must
/// run once, behind a mark ([`once`](isoloom_vagrant) guards) or its `creates`.
const CONVERGE: &[&str] = &[
    "addresses",
    "constraints",
    "disk",
    "data disks",
    "settings",
    "hosts",
    "routes",
    "default route",
    "forwarding",
    "ipv6",
    "trunks",
    "tc",
    "volumes",
    "project",
    "certificate authorities",
    "files",
    "secrets",
    "inputs",
    "configure",
    "before reboot",
    "reboot",
    "after reboot",
    "command",
    "setup",
    "no internet",
    "ready",
    "checks script",
    "checks",
    "router",
    "toolbox",
    "controller",
    "ansible",
    "online",
    "halt",
    "environment network",
];

#[test]
fn vagrant_provision_runs_again_safely() -> Outcome {
    let files = every_vagrantfile()?;
    assert!(files.len() > 10, "the examples render: {}", files.len());
    let mut once = 0;
    for (example, file) in &files {
        for (define, provisioners) in defines(file) {
            for (name, text) in provisioners {
                let marked = text.contains("isoloom_sum=") || text.contains("$isoloomSum");
                let created = text.contains("[ -e '") || text.contains("if (-not (Test-Path -LiteralPath");
                let windows_routes = name == "routes" && text.contains("route -p add");
                if windows_routes || !(CONVERGE.contains(&name.as_str()) || name.starts_with("wait for ")) {
                    assert!(marked || created, "{example}/{define}: {name} runs again on every provisioning:\n{text}");
                    once += 1;
                }
            }
        }
    }
    assert!(once > 10, "the project scripts and shared volumes run once: {once}");
    Ok(())
}

#[test]
fn the_readiness_check_is_each_machines_last_provisioner_and_its_failure_fails_vagrant_up() -> Outcome {
    for (example, file) in every_vagrantfile()? {
        let env = plan(&example)?;
        for (define, provisioners) in defines(&file) {
            if !env.machines.contains_key(&define) {
                continue;
            }
            let (last, text) = provisioners.last().ok_or(format!("{example}/{define}: no provisioner"))?;
            assert_eq!(last, "ready", "{example}/{define}");
            let healthcheck = env.machines.get(&define).is_some_and(|m| m.process.health.is_some());
            if healthcheck {
                assert!(
                    text.contains("exit 1") || text.contains("throw "),
                    "{example}/{define}: an unhealthy machine fails its provisioner:\n{text}"
                );
            }
        }
    }
    Ok(())
}

/// A key the runner gives that isn't one is refused when the Vagrantfile loads, never put in a name.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn a_malformed_key_is_refused() -> Outcome {
    needs(&["ruby"])?;
    let scratch = tempfile::Builder::new().prefix("isoloom-bad-key-").tempdir()?;
    let err = vagrant_host_names("solo-web", scratch.path(), Some("Bad Key")).unwrap_err().to_string();
    assert!(err.contains("ISOLOOM_KEY"), "{err}");
    Ok(())
}

#[test]
fn each_vm_with_checks_has_them_installed_for_vagrant_ssh() -> Outcome {
    for example in ["segmented", "corp-ad-basics", "ansible-pair"] {
        let file = vagrantfile(example)?;
        let runners: Vec<String> = render(example)?
            .into_iter()
            .filter_map(|f| f.path.strip_prefix(".isoloom/vagrant/checks/")?.strip_suffix(".sh").map(str::to_owned))
            .collect();
        assert!(!runners.is_empty(), "{example} has checks");
        let header: String = file.lines().take_while(|l| l.starts_with('#')).collect::<Vec<_>>().join("\n");
        assert!(
            header.contains("# Checks: vagrant ssh <vm> -c 'sudo sh /var/lib/isoloom/checks.sh', for each of"),
            "{example}: {header}"
        );
        assert!(!header.contains("run.json") && !header.contains("runner"), "{example}: {header}");
        for (define, provisioners) in defines(&file) {
            let names: Vec<&str> = provisioners.iter().map(|(n, _)| n.as_str()).collect();
            let has = names.contains(&"checks script");
            assert_eq!(has, runners.contains(&define), "{example}/{define}: {names:?}");
            if has {
                assert!(header.contains(&define), "{example}: {define} in the header: {header}");
                let (_, text) = provisioners.iter().find(|(n, _)| n == "checks script").ok_or("checks script")?;
                assert!(
                    text.contains(&format!("File.read(File.join(__dir__, \"checks/{define}.sh\"))"))
                        && text.contains("mv -f /var/lib/isoloom/checks.sh.new /var/lib/isoloom/checks.sh")
                        && !text.contains("run: \"never\""),
                    "installed at every provisioning: {text}"
                );
            }
        }
    }
    Ok(())
}

// Every libvirt network a VM joins exists when the VM starts: vagrant-libvirt creates it.
// vagrant-libvirt (`create_networks.rb`, `handle_ip_option`) finds a private network that has an
// `ip:` by its address block first, unless its forward mode is `veryisolated`, and only then by
// its name. When the computer already has a network on the block under another name (another
// copy of the environment, another environment on the same block, one left behind), it takes
// that one, makes nothing, and the VM's interface still names the network the Vagrantfile gave:
// `virDomainCreate` fails with "Network not found". The tests below read each VM's
// `private_network` lines from the rendered Vagrantfile and follow vagrant-libvirt's lookup on a
// computer that already holds every one of the environment's blocks under other names.

/// A Ruby call's keyword arguments (`key: value`), each value's text as written: a string's
/// contents (its `#{…}` left as it is), any other value as its literal.
/// Keyword `key` of `kw`, empty when it has none.
fn field<'k>(kw: &'k std::collections::BTreeMap<String, String>, key: &str) -> &'k str {
    kw.get(key).map_or("", String::as_str)
}

fn keywords(call: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut rest = call;
    while let Some(after_comma) = rest.find(", ").and_then(|i| rest.get(i + 2..)) {
        let Some((key, after)) = after_comma.split_once(": ") else { break };
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            rest = after_comma;
            continue;
        }
        let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
            string_literal(quoted)
        } else {
            let end = after.find(", ").unwrap_or(after.len());
            (after.get(..end).unwrap_or(after).trim().to_owned(), after.get(end..).unwrap_or(""))
        };
        out.insert(key.to_owned(), value);
        rest = tail;
    }
    out
}

/// The string literal starting at `s` (after its opening quote): its contents, and what follows
/// its closing quote (the first `"` not escaped and not inside a `#{…}`).
fn string_literal(s: &str) -> (String, &str) {
    let (mut value, mut depth, mut escaped) = (String::new(), 0_u32, false);
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '#' if chars.peek().is_some_and(|(_, n)| *n == '{') => depth += 1,
            '}' if depth > 0 => depth -= 1,
            '"' if depth == 0 => return (value, s.get(i + 1..).unwrap_or("")),
            _ => {}
        }
        value.push(c);
    }
    (value, "")
}

/// Each VM's private networks as libvirt sees them: (VM, the network's keyword arguments).
fn libvirt_networks(vagrantfile: &str) -> Vec<(String, std::collections::BTreeMap<String, String>)> {
    let file = isoloom_test_support::ruby_blocks(vagrantfile);
    let mut out = Vec::new();
    for vm in file.find("config.vm.define ") {
        let name = vm.head.split('"').nth(1).unwrap_or_default().to_owned();
        for call in vm.statements("m.vm.network \"private_network\"") {
            let kw = keywords(call);
            if kw.contains_key("libvirt__network_name") {
                out.push((name.clone(), kw));
            }
        }
    }
    out
}

/// The network vagrant-libvirt attaches the VM to (`Ok`), or why the VM won't start (`Err`), on a
/// computer whose networks are `host` (name → block).
fn attached(kw: &std::collections::BTreeMap<String, String>, host: &std::collections::BTreeMap<String, String>) -> Result<String, String> {
    let name = field(kw, "libvirt__network_name");
    let veryisolated = kw.get("libvirt__forward_mode").is_some_and(|m| m == "veryisolated");
    if !veryisolated {
        let block = block_of(field(kw, "ip"), field(kw, "netmask"));
        if let Some((other, _)) = host.iter().find(|(other, b)| **b == block && other.as_str() != name) {
            return Err(format!("{block} is already {other}'s: nothing makes `{name}`, which the VM's interface names"));
        }
    }
    // Found by its name, or made under it.
    Ok(name.to_owned())
}

/// The block `ip`/`netmask` is in, as `a.b.c.d/n`.
fn block_of(ip: &str, netmask: &str) -> String {
    // Unparsed, a block reads 0.0.0.0/0, which every crowded computer holds: the test fails.
    let ip: std::net::Ipv4Addr = ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    let mask: std::net::Ipv4Addr = netmask.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    let bits = u32::from(mask);
    format!("{}/{}", std::net::Ipv4Addr::from(u32::from(ip) & bits), bits.count_ones())
}

/// The computer as a run finds it: every block of `networks` already held by a network of
/// another name.
fn crowded(networks: &[(String, std::collections::BTreeMap<String, String>)]) -> std::collections::BTreeMap<String, String> {
    networks
        .iter()
        .map(|(_, kw)| {
            (
                format!("{}-elsewhere", field(kw, "libvirt__network_name")),
                block_of(field(kw, "ip"), field(kw, "netmask")),
            )
        })
        .collect()
}

/// Every VM of `vagrantfile` starts on a computer that already holds its networks' blocks under
/// other names: each joins the network the Vagrantfile names, made for it.
fn every_network_is_made(vagrantfile: &str) {
    let networks = libvirt_networks(vagrantfile);
    assert!(!networks.is_empty(), "no libvirt network in:\n{vagrantfile}");
    let host = crowded(&networks);
    for (vm, kw) in &networks {
        assert_eq!(attached(kw, &host).as_ref(), Ok(&field(kw, "libvirt__network_name").to_owned()), "{vm}: {kw:?}");
    }
}

#[test]
fn a_multi_machine_labs_network_is_made_even_where_another_network_holds_its_block() -> Outcome {
    // supplier-portal-api and invoice-portal-api are both on 10.20.0.0/24.
    every_network_is_made(&vagrantfile("supplier-portal-api")?);
    every_network_is_made(&vagrantfile("segmented")?);
    Ok(())
}

#[test]
fn a_one_machine_labs_network_is_made_too() -> Outcome {
    every_network_is_made(&vagrantfile("solo-web")?);
    Ok(())
}

#[test]
fn an_ipv6_network_is_made_by_its_name_with_its_block() -> Outcome {
    let vagrantfile = vagrantfile("dual-stack")?;
    every_network_is_made(&vagrantfile);
    for (vm, kw) in libvirt_networks(&vagrantfile) {
        assert!(kw.contains_key("libvirt__ipv6_address"), "{vm}: {kw:?}");
    }
    Ok(())
}

#[test]
fn the_lookup_this_test_follows_fails_on_a_network_found_by_its_block() {
    // The failure the run met: a network found by its block under another name.
    let kw: std::collections::BTreeMap<String, String> = [("libvirt__network_name", "isoloom-a-lab"), ("ip", "10.20.0.31"), ("netmask", "255.255.255.0")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let host: std::collections::BTreeMap<String, String> = [("isoloom-b-lab".to_owned(), "10.20.0.0/24".to_owned())].into_iter().collect();
    assert!(attached(&kw, &host).is_err());
    let mut isolated = kw;
    isolated.insert("libvirt__forward_mode".to_owned(), "veryisolated".to_owned());
    assert_eq!(attached(&isolated, &host), Ok("isoloom-a-lab".to_owned()));
}

#[test]
fn keywords_read_strings_with_interpolations_and_bare_values() {
    let kw =
        keywords("m.vm.network \"private_network\", ip: \"10.20.0.32\", virtualbox__intnet: \"isoloom-x-#{ISOLOOM_KEY}-lab\", libvirt__dhcp_enabled: false");
    assert_eq!(field(&kw, "ip"), "10.20.0.32");
    assert_eq!(field(&kw, "virtualbox__intnet"), "isoloom-x-#{ISOLOOM_KEY}-lab");
    assert_eq!(field(&kw, "libvirt__dhcp_enabled"), "false");
}

/// After [`VAGRANT_STAND_IN_RB`]: fires every VM's UTM triggers: before `Boot` (the adapters, the `osascript` call recorded instead of run) and
/// after `CheckGuestAdditions` (the guest's addresses, its script recorded). Prints each QEMU
/// argument the adapters set (`arg <vm> <argument>`) and each address the guest sets
/// (`address <vm> <mac> <address>`).
const UTM_TRIGGERS_RB: &str = r#"
require "open3"
$vm = nil
ok = Struct.new(:success?).new(true)
Open3.define_singleton_method(:capture2e) do |*argv, stdin_data: nil|
  # osascript - <vm id> <mark> <argument>...
  argv.drop(4).each { |a| puts "arg #{$vm} #{a}" }
  ["", ok]
end
IsoloomUtm.define_singleton_method(:address) do |_machine, script|
  script.scan(/^set_address (\S+) (\S+)/) { |mac, ip| puts "address #{$vm} #{mac} #{ip}" }
end
$triggers.keys.map(&:first).uniq.each do |vm|
  $vm = vm
  machine = Struct.new(:name, :id).new(vm.to_sym, "id-#{vm}")
  [["Boot", :before], ["CheckGuestAdditions", :after]].each do |action, at|
    code = $triggers[[vm, action, at]] or next
    code.call(nil, machine)
  end
end
"#;

/// What [`UTM_TRIGGERS_RB`] prints for `example`: each VM's adapters, as (VM, network id, MAC),
/// and the addresses its guest sets, as (VM, MAC, address).
/// A VM, a network id or MAC, and a MAC or address: one line of [`UTM_TRIGGERS_RB`].
type Triple = (String, String, String);

fn utm_adapters(example: &str) -> Outcome<(Vec<Triple>, Vec<Triple>)> {
    let scratch = tempfile::Builder::new().prefix("isoloom-utm-nics-").tempdir()?;
    let (vagrantfile, mut ruby) = loadable(example, scratch.path())?;
    ruby.arg("-e").arg([VAGRANT_STAND_IN_RB, UTM_TRIGGERS_RB].concat()).arg(vagrantfile);
    let out = printed(ruby)?;
    // Each VM's netdevs (id → network id), then its cards (netdev, MAC).
    let mut netdevs = std::collections::BTreeMap::new();
    let (mut cards, mut addresses) = (Vec::new(), Vec::new());
    for line in out.lines() {
        let mut words = line.splitn(3, ' ');
        let (kind, vm, rest) = (words.next(), words.next().unwrap_or_default().to_owned(), words.next().unwrap_or_default());
        match kind {
            Some("arg") => {
                let (device, options) = rest.split_once(' ').ok_or("an argument without a value")?;
                let opts: std::collections::BTreeMap<&str, &str> = options.split(',').filter_map(|o| o.split_once('=')).collect();
                match device {
                    "-netdev" => {
                        assert!(options.starts_with("vmnet-host,"), "{vm}: {rest}");
                        netdevs.insert(
                            (vm.clone(), opts.get("id").ok_or("id missing")?.to_string()),
                            opts.get("net-uuid").ok_or("net-uuid missing")?.to_string(),
                        );
                    }
                    "-device" => {
                        let uuid = netdevs
                            .get(&(vm.clone(), opts.get("netdev").ok_or("netdev missing")?.to_string()))
                            .ok_or("a card before its netdev")?;
                        cards.push((vm, uuid.clone(), opts.get("mac").ok_or("mac missing")?.to_string()));
                    }
                    other => return Err(format!("unexpected argument {other}").into()),
                }
            }
            Some("address") => {
                let (mac, ip) = rest.split_once(' ').ok_or("an address without its MAC")?;
                addresses.push((vm, mac.to_owned(), ip.to_owned()));
            }
            _ => {}
        }
    }
    Ok((cards, addresses))
}

/// On UTM each network of the environment is a vmnet network of its own: every VM on it gets a card on the same
/// network id, a network's VMs share none with another network, and each guest sets its address
/// on the card with that MAC. Run on segmented (three networks and the router between them) and
/// supplier-portal-api (three VMs on one network, the controller among them). The Ruby itself
/// runs here when `ruby` is on the PATH (CI has it).
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn every_vm_of_a_network_gets_a_card_on_the_same_vmnet_network_on_utm() -> Outcome {
    needs(&["ruby"])?;
    for example in ["segmented", "supplier-portal-api"] {
        let env = plan(example)?;
        let (cards, addresses) = utm_adapters(example)?;
        let mut ids: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> = std::collections::BTreeMap::new();
        for vm in env.vms() {
            let label = vm.name;
            for (net, ip) in &vm.plan.addresses {
                // The card the guest sets this network's address on.
                let at = format!("{ip}/{}", env.block(net).prefix());
                let (_, mac, _) = addresses
                    .iter()
                    .find(|(v, _, a)| v == label && *a == at)
                    .ok_or_else(|| format!("{example}: {label} sets no {at} ({net}): {addresses:?}"))?;
                let (_, uuid, _) = cards
                    .iter()
                    .find(|(v, _, m)| v == label && m == mac)
                    .ok_or_else(|| format!("{example}: {label} has no card with {mac} ({net}): {cards:?}"))?;
                ids.entry(net.as_str()).or_default().insert(uuid.as_str());
            }
        }
        assert_eq!(ids.len(), env.networks.len(), "{example}: a network no VM is on: {ids:?}");
        for (net, uuids) in &ids {
            assert_eq!(uuids.len(), 1, "{example}: network {net}'s VMs are on different vmnet networks: {uuids:?}");
        }
        let distinct: std::collections::BTreeSet<_> = ids.values().flatten().collect();
        assert_eq!(distinct.len(), ids.len(), "{example}: two networks share a vmnet network: {ids:?}");
    }
    Ok(())
}

/// On VMware Desktop each adapter on the environment's networks is pinned to a PCI slot of its
/// own (`ethernet<n>.pcislotnumber`, from 1: the box's NAT adapter keeps its own), one per
/// `private_network` line of the VM: a slot VMware picked as the adapter was added moved the
/// guest's interfaces under other names, and closed its SSH session.
#[test]
fn every_vmware_adapter_on_a_network_has_a_pci_slot_of_its_own() -> Outcome {
    for example in ["segmented", "supplier-portal-api", "dmz-jump"] {
        let file = vagrantfile(example)?;
        let r = isoloom_test_support::ruby_blocks(&file);
        for vm in r.find("config.vm.define ") {
            let name = vm.head.split('"').nth(1).unwrap_or_default();
            let networks = vm.statements("m.vm.network \"private_network\"").len();
            let vmware = vm.find("m.vm.provider \"vmware_desktop\" do |");
            let vmware = vmware.first().ok_or_else(|| format!("{example}: {name} has no VMware block"))?;
            let slots: Vec<(String, String)> = vmware
                .statements("v.vmx[\"ethernet")
                .iter()
                .filter_map(|s| s.strip_prefix("v.vmx[\"ethernet")?.split_once(".pcislotnumber\"] = "))
                .map(|(n, slot)| (n.to_owned(), slot.trim_matches('"').to_owned()))
                .collect();
            let adapters: Vec<String> = slots.iter().map(|(n, _)| n.clone()).collect();
            let wanted: Vec<String> = (1..=networks).map(|n| n.to_string()).collect();
            assert_eq!(adapters, wanted, "{example}: {name}'s adapters");
            let distinct: std::collections::BTreeSet<&str> = slots.iter().map(|(_, s)| s.as_str()).collect();
            assert_eq!(distinct.len(), slots.len(), "{example}: {name}: {slots:?}");
            assert!(
                slots.iter().all(|(_, s)| s.parse::<u32>().is_ok_and(|n| n >= 160)),
                "{example}: {name}: {slots:?}"
            );
        }
    }
    Ok(())
}

/// A port Isothrum keeps across operations comes back where it was: the forward (and Hyper-V's
/// relay) read `ISOTHRUM_HOST_PORT_<MACHINE>_<GUEST>_<PROTO>` first, then the runner's choice for
/// this run, then the published number, `auto_correct` kept.
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn a_kept_host_port_wins_over_the_runs_choice_and_the_published_one() -> Outcome {
    needs(&["ruby"])?;
    let file = vagrantfile("solo-web")?;
    let r = isoloom_test_support::ruby_blocks(&file);
    let web = r.block("config.vm.define \"web\"");
    let forwards = web.statements("m.vm.network \"forwarded_port\"");
    assert!(
        forwards
            .iter()
            .any(|f| f.contains("host: ENV.fetch(\"ISOTHRUM_HOST_PORT_WEB_80_TCP\", ") && f.ends_with("auto_correct: true")),
        "{forwards:?}"
    );
    let scratch = tempfile::Builder::new().prefix("isoloom-kept-ports-").tempdir()?;
    let chosen = r#"{"web/web": 28080}"#;
    for (kept, run, wanted) in [(Some("18080"), Some(chosen), 18080), (None, Some(chosen), 28080), (None, None, 8080)] {
        let (vagrantfile, mut ruby) = loadable("solo-web", scratch.path())?;
        ruby.arg("-e")
            .arg([VAGRANT_STAND_IN_RB, HYPERV_TRIGGERS_RB].concat())
            .arg(vagrantfile)
            .arg("web");
        match kept {
            Some(port) => ruby.env("ISOTHRUM_HOST_PORT_WEB_80_TCP", port),
            None => ruby.env_remove("ISOTHRUM_HOST_PORT_WEB_80_TCP"),
        };
        match run {
            Some(json) => ruby.env("ISOLOOM_HOSTPORTS", json),
            None => ruby.env_remove("ISOLOOM_HOSTPORTS"),
        };
        let out = printed(ruby)?;
        assert!(out.lines().any(|l| l == format!("forward 80 {wanted}")), "{kept:?} {run:?}: {out}");
    }
    Ok(())
}
