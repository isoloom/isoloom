//! Hyper-V's part of a Vagrantfile: the relay of each published port, the host port a forward
//! takes, and a disk size that grows the box's disk, never below its own.

use isoloom_test_support::{assert_every, needs};

use crate::testing::{Outcome, VAGRANT_STAND_IN_RB, loadable, printed, vagrantfile};

/// Hyper-V forwards no ports: each published TCP port is relayed from 127.0.0.1 to the VM at every
/// start, from the host port its `forwarded_port` line names (the runner's choice, else the
/// published one), and the relay goes with the VM. Alpine's controller keeps its hostname there:
/// setting one restarts every interface, which closed the SSH session setting it.
/// A machine's disk size grows the box's disk, never below its own: Vagrant's own disk setting
/// does so in VirtualBox's and VMware's blocks, but on Hyper-V it shrinks a VHDX (which Hyper-V
/// refuses when the box's disk is the bigger), so there it is Isoloom's step before the VM starts,
/// and the machine has no disk setting Hyper-V would read.
#[test]
fn a_disk_size_grows_the_box_disk_on_hyperv_and_never_shrinks_it() -> Outcome {
    let file = vagrantfile("segmented")?;
    let r = isoloom_test_support::ruby_blocks(&file);
    let cache = r.block("config.vm.define \"cache\"");
    let primary = "o.vm.disk :disk, size: \"10GB\", primary: true";
    assert!(
        cache.statements("m.vm.disk").is_empty(),
        "no disk setting every provider reads: {:?}",
        cache.statements
    );
    for provider in ["virtualbox", "vmware_desktop"] {
        let block = cache.block(&format!("m.vm.provider \"{provider}\" do |v, o|"));
        assert!(block.statements.iter().any(|s| s == primary), "{provider}: {:?}", block.statements);
    }
    let hyperv = cache.block("m.vm.provider \"hyperv\" do |v, o|");
    assert!(!hyperv.statements.iter().any(|s| s.contains("vm.disk")), "{:?}", hyperv.statements);
    let starting = cache.block("m.trigger.before :\"VagrantPlugins::HyperV::Action::StartInstance\"");
    assert_every!(starting.statements("t.ruby"), |s: &&str| s.contains("IsoloomHyperV.grow(machine, 10)"));
    assert_eq!(starting.statements("t.ruby").len(), 1, "{:?}", starting.statements);
    Ok(())
}

#[test]
fn hyperv_relays_published_ports_and_alpine_keeps_its_hostname() -> Outcome {
    let file = vagrantfile("solo-web")?;
    let r = isoloom_test_support::ruby_blocks(&file);
    let web = r.block("config.vm.define \"web\"");
    let host = "ENV.fetch(\"HOST_PORT_WEB_80_TCP\", ISOLOOM_HOSTPORTS.fetch(\"web/web\", 8080)).to_i";
    assert_eq!(
        web.statements("m.vm.network \"forwarded_port\""),
        [format!(
            "m.vm.network \"forwarded_port\", guest: 80, host: {host}, host_ip: \"127.0.0.1\", auto_correct: true"
        )],
        "the forward"
    );
    let started = web
        .block("m.trigger.after :\"VagrantPlugins::HyperV::Action::WaitForIPAddress\"")
        .block("t.ruby do |_, machine|");
    assert!(
        started.statements.contains(&format!("IsoloomHyperV.ports(machine, [[80, {host}]])")),
        "the relay at start: {:?}",
        started.statements
    );
    let deleted = web.block("m.trigger.after :\"VagrantPlugins::HyperV::Action::DeleteVM\"");
    assert_eq!(deleted.statements("t.ruby").len(), 1, "{:?}", deleted.statements);
    assert_every!(deleted.statements("t.ruby"), |s: &&str| s
        .contains(&format!("IsoloomHyperV.unports([[80, {host}]])")));
    let controller = r.block("config.vm.define \"isoloom-controller\"");
    let hyperv = controller.block("m.vm.provider \"hyperv\" do |v, o|");
    assert!(hyperv.statements.iter().any(|s| s == "o.vm.hostname = nil"), "{:?}", hyperv.statements);
    Ok(())
}

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

/// A port the runner keeps across operations comes back where it was: the forward (and Hyper-V's
/// relay) read `HOST_PORT_<MACHINE>_<GUEST>_<PROTO>` first, then the runner's choice for
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
            .any(|f| f.contains("host: ENV.fetch(\"HOST_PORT_WEB_80_TCP\", ") && f.ends_with("auto_correct: true")),
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
            Some(port) => ruby.env("HOST_PORT_WEB_80_TCP", port),
            None => ruby.env_remove("HOST_PORT_WEB_80_TCP"),
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
