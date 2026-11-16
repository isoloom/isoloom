//! Hyper-V's part of a Vagrantfile: the relay of each published port, and the host port a
//! forward takes.

use isoloom_test_support::needs;

use crate::testing::{Outcome, VAGRANT_STAND_IN_RB, loadable, printed, vagrantfile};

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
