//! QEMU's part of a Vagrantfile, run on a stand-in for Vagrant: each network's hub, its ports, its
//! start and stop, and the shared folders' virtiofsd.

use std::path::Path;
use std::process::Command;

use isoloom_test_support::{assert_every, needs};

use crate::testing::{Outcome, loadable, printed};

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
    let left = scratch.no_hub_left().err().ok_or("a running hub is a failure")?.to_string();
    assert!(
        left.starts_with("QEMU hubs outlived the test: ") && left.contains(&*pid_file.to_string_lossy()),
        "{left}"
    );
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
            assert_every!(&ports, |(_, p): &&(String, u16)| (20000..40000).contains(p));
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
    // The record (JSON, which YAML reads): every VM's port, the joining one's included, and the
    // spares it didn't take.
    let kept = isoloom_test_support::yaml(&std::fs::read_to_string(&record)?);
    let recorded: Vec<(String, String)> = isoloom_test_support::yaml_keys_at(&kept, "vms")
        .into_iter()
        .map(|vm| {
            let port = isoloom_test_support::yaml_str(&kept, &format!("vms.{vm}"));
            (vm, port)
        })
        .collect();
    let wanted: Vec<(String, String)> = now.iter().map(|(vm, p)| (vm.clone(), p.to_string())).collect();
    assert_eq!(recorded, wanted, "{kept:?}");
    assert_eq!(isoloom_test_support::yaml_strs(&kept, "spare"), spares[1..], "{kept:?}");
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
    use isoloom_render::backend::Backend;
    use isoloom_vagrant::VagrantBackend;

    use crate::testing::parsed;

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
