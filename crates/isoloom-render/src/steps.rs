//! Each set-up step of a VM ([`Step`]) written once: as a root shell script for Linux, and as
//! PowerShell where Windows has the step. A VM backend only picks how the scripts reach the
//! machine (a Vagrant provisioner, cloud-init, `remote-exec`, WinRM), so every target sets a
//! machine up the same way.

use std::fmt::Write as _;

use isoloom_plan::Environment;
use isoloom_plan::lifecycle::{RunKind, SharedRole, Step};

use crate::shell::quote;
use crate::{address, constraints, machine_setup, router, trunks};

/// Where the project is unpacked on a VM.
pub const PROJECT_DIR: &str = "/opt/isoloom";
/// The project's archive, uploaded before the steps run.
pub const PROJECT_ARCHIVE: &str = "/tmp/isoloom-project.tgz";
/// The inputs, uploaded before the steps run (`KEY=value` lines).
pub const INPUTS_FILE: &str = "/tmp/isoloom-inputs.env";
/// Where the inputs are kept when a reboot comes (`/tmp` is emptied at boot).
pub const KEPT_INPUTS_FILE: &str = "/var/lib/isoloom/inputs.env";
/// Installs Docker from Docker's own script, unless it is there (a root script): the Docker
/// hosts of `docker-vm` and `cloud-docker`.
pub const INSTALL_DOCKER: &str = "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh";
/// Seconds a machine waits for one it depends on.
pub const WAIT_TIMEOUT: u32 = 900;

/// This boot's id: it changes at each boot.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// `step` of `machine` as a root shell script for a Linux VM; `None` when it has nothing to do
/// there.
pub fn linux(env: &Environment, machine: &str, step: &Step) -> Option<String> {
    match step {
        Step::Prelude => constraints::linux_prelude(env.spec()),
        Step::DiskGrow => Some(disk_grow()),
        Step::Settings => settings(env, machine),
        Step::Hosts { entries } => hosts(entries),
        Step::Routes { routes, default } => {
            let mut lines: Vec<String> = routes.iter().map(|r| format!("ip route replace {} via {}", r.cidr, r.via)).collect();
            lines.extend(default.map(|gw| format!("ip route replace default via {gw}")));
            lines_or_none(&lines)
        }
        Step::Ipv6 { addresses, routes } => Some(crate::ipv6::linux(addresses, routes)),
        Step::Trunks => trunk_commands(env, machine),
        Step::Volumes { paths } => (!paths.is_empty()).then(|| format!("mkdir -p {}\n", paths.iter().map(|p| quote(p)).collect::<Vec<_>>().join(" "))),
        Step::SharedVolume { volume, path, role } => Some(shared_volume(volume, path, role)),
        Step::Project => Some(format!(
            "mkdir -p {PROJECT_DIR} && tar -xzf {PROJECT_ARCHIVE} -C {PROJECT_DIR} --strip-components=1 && rm -f {PROJECT_ARCHIVE}\n"
        )),
        Step::Inputs { persist } => (*persist).then(|| format!("mkdir -p /var/lib/isoloom && install -m 600 {INPUTS_FILE} {KEPT_INPUTS_FILE}\n")),
        Step::WaitFor { machine: dep, ports } => Some(format!("{}\n", router::wait_for(dep, ports, WAIT_TIMEOUT))),
        Step::Run { kind: RunKind::PowerShell, .. } => None,
        Step::Run { path, kind } => Some(run(env, machine, path, *kind)),
        Step::Reboot => Some(reboot()),
        Step::Egress => Some(egress(env)),
        Step::Tc => tc(env, machine),
        Step::Ready => Some("mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready\n".into()),
    }
}

/// `step` of `machine` as PowerShell for a Windows VM; `None` when Windows has no such step
/// (or nothing to do for it).
pub fn windows(env: &Environment, machine: &str, step: &Step) -> Option<String> {
    let ps = |s: &str| format!("'{}'", s.replace('\'', "''"));
    match step {
        Step::Settings => env.spec().machines.get(machine).and_then(machine_setup::windows),
        Step::Hosts { entries } if !entries.is_empty() => {
            let lines = entries
                .iter()
                .map(|(a, names)| ps(&format!("{a} {}", names.join(" "))))
                .collect::<Vec<_>>()
                .join(", ");
            // Each line once: running the step again adds nothing.
            Some(format!(
                "$h = \"$env:SystemRoot\\System32\\drivers\\etc\\hosts\"; foreach ($l in @({lines})) {{ if (-not (Select-String -Path $h -SimpleMatch $l -Quiet)) {{ Add-Content -Path $h -Value $l }} }}"
            ))
        }
        Step::Routes { routes, .. } if !routes.is_empty() => Some(
            routes
                .iter()
                .map(|r| format!("route -p add {} MASK {} {}", network_of(&r.cidr), mask_of(&r.cidr), r.via))
                .collect::<Vec<_>>()
                .join("; "),
        ),
        Step::Ipv6 { addresses, routes } => Some(crate::ipv6::windows(addresses, routes)),
        Step::Project => Some(format!(
            "$archive = 'C:\\Windows\\Temp\\isoloom-project.tgz'; $dest = 'C:\\isoloom'; $strip = 1\n{}",
            crate::UNTAR_PS1
        )),
        Step::Run {
            path,
            kind: RunKind::PowerShell,
        } => Some(format!(
            "Set-Location C:\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File {}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
            ps(path)
        )),
        Step::Ready => Some("New-Item -ItemType Directory -Force C:\\isoloom | Out-Null; Set-Content C:\\isoloom\\ready ready".into()),
        Step::DiskGrow => Some(GROW_ROOT_PS1.into()),
        Step::WaitFor { machine: dep, ports } if !ports.is_empty() => Some(windows_wait_for(dep, ports)),
        _ => None,
    }
}

/// The lines joined as a script, or `None` when there are none.
fn lines_or_none(lines: &[String]) -> Option<String> {
    (!lines.is_empty()).then(|| {
        lines.iter().fold(String::new(), |mut s, l| {
            let _ = writeln!(s, "{l}");
            s
        })
    })
}

/// Grows the root file system into a disk made bigger than the image's: its partition when it
/// is the disk's last, LVM's physical and logical volumes (Ubuntu's images), then the file
/// system (ext4 or XFS). Best effort and idempotent: with nothing to grow, nothing changes.
fn disk_grow() -> String {
    GROW_ROOT_SH.into()
}

const GROW_ROOT_SH: &str = r#"set -e
part_grow() {
  dev=$(readlink -f "$1")
  disk=/dev/$(lsblk -no PKNAME "$dev" | head -n1)
  num=$(cat "/sys/class/block/$(basename "$dev")/partition")
  if command -v growpart >/dev/null; then growpart "$disk" "$num" >/dev/null || true
  else echo ", +" | sfdisk --no-reread -N "$num" "$disk" >/dev/null 2>&1 && partx -u "$disk" || true; fi
}
fs_grow() { resize2fs "$1" >/dev/null 2>&1 || xfs_growfs / >/dev/null 2>&1 || true; }
src=$(findmnt -n -o SOURCE /)
case "$(lsblk -no TYPE "$src" | head -n1)" in
  lvm)
    vg=$(lvs --noheadings -o vg_name "$src" | tr -d ' ')
    for pv in $(pvs --noheadings -o pv_name -S vg_name="$vg" | tr -d ' '); do part_grow "$pv"; pvresize "$pv" >/dev/null; done
    lvextend -l +100%FREE "$src" >/dev/null 2>&1 || true
    fs_grow "$src" ;;
  part) part_grow "$src"; fs_grow "$src" ;;
esac
"#;

/// The same on Windows: C: to the largest size its disk allows.
const GROW_ROOT_PS1: &str = "$max = (Get-PartitionSupportedSize -DriveLetter C).SizeMax; if ((Get-Partition -DriveLetter C).Size -lt $max) { Resize-Partition -DriveLetter C -Size $max }";

/// Waits until `machine` (a name or an address) answers on each of `ports`, up to
/// [`WAIT_TIMEOUT`] seconds, in PowerShell.
fn windows_wait_for(machine: &str, ports: &[u16]) -> String {
    let ports = ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
    let target = crate::ps(machine);
    let failed = crate::ps(&format!("{machine} didn't answer on port "));
    let answers = crate::ps(&format!("{machine} answers"));
    format!(
        "$t = (Get-Date).AddSeconds({WAIT_TIMEOUT}); foreach ($p in @({ports})) {{ while (-not (Test-NetConnection {target} -Port $p -WarningAction SilentlyContinue).TcpTestSucceeded) {{ if ((Get-Date) -gt $t) {{ throw ({failed} + $p) }}; Start-Sleep 5 }} }}; {answers}"
    )
}

/// The machine's environment, sysctls and ulimits: `machine_setup`'s script without the shared
/// volumes, which are steps of their own.
fn settings(env: &Environment, machine: &str) -> Option<String> {
    let mut m = env.spec().machines.get(machine)?.clone();
    m.volumes.clear();
    machine_setup::linux(env.spec(), machine, &m)
}

/// The other machines by name, appended to `/etc/hosts`.
fn hosts(entries: &[(std::net::Ipv4Addr, Vec<String>)]) -> Option<String> {
    let lines: Vec<String> = entries.iter().map(|(a, names)| quote(&format!("{a} {}", names.join(" ")))).collect();
    // Each line once: running the step again adds nothing.
    (!lines.is_empty()).then(|| {
        format!(
            "for l in {}; do grep -qxF \"$l\" /etc/hosts || printf '%s\\n' \"$l\" >> /etc/hosts; done\n",
            lines.join(" ")
        )
    })
}

/// The machine's trunks, built inside it.
fn trunk_commands(env: &Environment, machine: &str) -> Option<String> {
    let spec = env.spec();
    let lines: Vec<String> = trunks::vm_trunks(spec)
        .iter()
        .filter(|t| t.machine == machine)
        .flat_map(|t| trunks::vm_commands(spec, t, |n, o| address(spec, n, o)))
        .collect();
    lines_or_none(&lines)
}

/// A shared volume: its owner serves the directory over NFS to the machines mounting it; each
/// of them mounts it at its own path (at boot too), waiting up to 15 minutes for the owner.
fn shared_volume(volume: &str, path: &str, role: &SharedRole) -> String {
    let mut sh = format!("{}\nmkdir -p {}\n", machine_setup::PKG, quote(path));
    match role {
        SharedRole::Serve { clients } => {
            let clients = clients
                .iter()
                .map(|c| format!("{c}(rw,sync,no_subtree_check,no_root_squash)"))
                .collect::<Vec<_>>()
                .join(" ");
            let tag = quote(&format!("# isoloom-volume-{volume}"));
            let _ = writeln!(
                sh,
                "if command -v apt-get >/dev/null; then pkg nfs-kernel-server; else pkg nfs-utils; fi\n\
                 grep -v -F {tag} /etc/exports > /etc/exports.isoloom 2>/dev/null || true\n\
                 printf '%s %s %s\\n' {} {} {tag} >> /etc/exports.isoloom && mv /etc/exports.isoloom /etc/exports\n\
                 systemctl enable --now nfs-server 2>/dev/null || systemctl enable --now nfs-kernel-server\n\
                 exportfs -ra",
                quote(path),
                quote(&clients),
            );
        }
        SharedRole::Mount { source, owner } => {
            let fstab = format!("{source} {path} nfs defaults,_netdev 0 0");
            let _ = writeln!(
                sh,
                "if command -v apt-get >/dev/null; then pkg nfs-common; else pkg nfs-utils; fi\n\
                 grep -qF {} /etc/fstab || printf '%s\\n' {} >> /etc/fstab\n\
                 i=0; until mountpoint -q {p} || mount {p} 2>/dev/null; do i=$((i+1)); if [ $i -ge 180 ]; then echo {} >&2; exit 1; fi; sleep 5; done",
                quote(&format!("{source} ")),
                quote(&fstab),
                quote(&format!("volume {volume}: {owner} doesn't serve it (NFS)")),
                p = quote(path),
            );
        }
    }
    sh
}

/// One `provision` step, from the project folder, with the machine's inputs in its environment.
fn run(env: &Environment, machine: &str, path: &str, kind: RunKind) -> String {
    let m = env.machines.get(machine);
    let inputs = match m {
        Some(m) if !m.inputs.is_empty() => {
            let kept = m.lifecycle.iter().any(|s| matches!(s, Step::Inputs { persist: true }));
            format!("set -a; . {}; set +a; ", if kept { KEPT_INPUTS_FILE } else { INPUTS_FILE })
        }
        _ => String::new(),
    };
    match kind {
        // A Windows script: rendered by `windows` only.
        RunKind::PowerShell => String::new(),
        RunKind::Shell => format!("cd {PROJECT_DIR} && sh -c {}\n", quote(&format!("{inputs}sh {path}"))),
        // Installed with the machine's own package manager; its inventory is the machine, by
        // its name (a play may name it), run locally.
        RunKind::Playbook => format!(
            "{}\ncommand -v ansible-playbook >/dev/null || pkg ansible-core\n\
             cd {PROJECT_DIR} && sh -c {}\n",
            machine_setup::PKG,
            quote(&format!("{inputs}ansible-playbook -c local -i {machine}, {path}"))
        ),
    }
}

/// Records this boot and restarts; the backend runs the next steps once the machine is back,
/// starting with [`reboot_check`].
fn reboot() -> String {
    format!("{}(sleep 2 && reboot) >/dev/null 2>&1 &\n", record_boot())
}

/// Records this boot's id, for [`reboot_check`]: a transport that restarts the machine itself
/// (Terraform's `remote-exec`) runs it before restarting.
pub fn record_boot() -> String {
    format!("mkdir -p /var/lib/isoloom && cp {BOOT_ID} /var/lib/isoloom/boot-id\n")
}

/// After a reboot step: fails when the machine didn't actually restart.
pub fn reboot_check() -> String {
    format!("if [ \"$(cat {BOOT_ID})\" = \"$(cat /var/lib/isoloom/boot-id)\" ]; then echo 'the reboot step: the machine did not restart' >&2; exit 1; fi\n")
}

/// Offline: no new connection leaves the environment's networks (each network's block is
/// allowed, everything else new is dropped; the loopback stays open), now and at every boot
/// ([`boot::at_boot`](crate::boot::at_boot) loads the rules). Run again, it replaces its table:
/// nftables comes from the machine's package manager (Alpine's too: the controller's default box).
fn egress(env: &Environment) -> String {
    let lab = env.networks.values().map(|n| n.cidr.as_str()).collect::<Vec<_>>().join(", ");
    let lab6 = crate::ipv6::egress_rule(env);
    let load = crate::boot::at_boot(
        "egress",
        "No new connection leaves the isoloom networks",
        "nft -f /etc/isoloom-egress.nft\n",
        false,
        &[],
    );
    format!(
        "{}\ncommand -v nft >/dev/null || if command -v apk >/dev/null; then apk add -q --no-cache nftables; else pkg nftables; fi\n\
         printf '%s\\n' 'table inet isoloom-egress' 'delete table inet isoloom-egress' 'table inet isoloom-egress {{' '  chain output {{' '    type filter hook output priority 0; policy accept;' '    oifname \"lo\" accept' '    ip daddr != {{ {lab} }} ct state new drop'{lab6} '  }}' '}}' > /etc/isoloom-egress.nft\n{load}",
        machine_setup::PKG
    )
}

/// Link impairment on the machine's own interfaces.
fn tc(env: &Environment, machine: &str) -> Option<String> {
    let spec = env.spec();
    let m = spec.machines.get(machine)?;
    router::machine_tc_script(spec, m, |n, o| address(spec, n, o))
}

/// The network address of a block (`10.1.0.0/24` gives `10.1.0.0`).
fn network_of(cidr: &str) -> &str {
    cidr.split('/').next().unwrap_or(cidr)
}

/// The dotted netmask of a block (`10.1.0.0/24` gives `255.255.255.0`).
fn mask_of(cidr: &str) -> std::net::Ipv4Addr {
    let len: u32 = cidr.split('/').nth(1).and_then(|l| l.parse().ok()).unwrap_or(32).min(32);
    std::net::Ipv4Addr::from(if len == 0 { 0 } else { u32::MAX << (32 - len) })
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_plan::lifecycle::Step;
    use isoloom_spec::model::Target;

    const SPEC: &str = r"
version: 1
name: steps
networks:
  front: { cidr: 10.84.0.0/24 }
  back: { cidr: 10.85.0.0/24, internet: false }
volumes:
  shared: { owner: writer }
reach:
  - { from: front, to: back }
machines:
  writer:
    networks: { front: 10, back: 10 }
    aliases: [store]
    volumes: { shared: /srv/shared }
    vm: { os: debian-12, provision: [steps/write.sh] }
  reader:
    networks: { back: 11 }
    volumes: { shared: /data/shared }
    vm: { os: debian-12, provision: [steps/look.sh] }
  web:
    networks: { front: 12 }
    vm: { os: debian-12, provision: [steps/web.sh] }
";

    fn env() -> Environment {
        let spec = isoloom_spec::parse(SPEC).expect("the test spec parses");
        assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
        Environment::build(&spec, Target::Vagrant, None)
    }

    fn rendered(env: &Environment, machine: &str, pick: impl Fn(&Step) -> bool) -> String {
        let step = env.machines[machine].lifecycle.iter().find(|s| pick(s)).expect("the step is planned");
        super::linux(env, machine, step).expect("the step renders")
    }

    #[test]
    fn routes_go_through_the_router() {
        let env = env();
        let sh = rendered(&env, "web", |s| matches!(s, Step::Routes { .. }));
        assert!(sh.contains("ip route replace 10.85.0.0/24 via "), "{sh}");
    }

    #[test]
    fn the_owner_serves_a_shared_volume() {
        let env = env();
        let sh = rendered(&env, "writer", |s| matches!(s, Step::SharedVolume { .. }));
        assert!(sh.contains("exportfs -ra"), "{sh}");
        let mount = rendered(&env, "reader", |s| matches!(s, Step::SharedVolume { .. }));
        assert!(mount.contains("/srv/shared") && mount.contains("nfs defaults"), "{mount}");
    }

    #[test]
    fn hosts_carry_aliases() {
        let env = env();
        let sh = rendered(&env, "reader", |s| matches!(s, Step::Hosts { .. }));
        assert!(sh.contains("writer store"), "{sh}");
        assert!(sh.contains("grep -qxF"), "each line once: {sh}");
    }

    #[test]
    fn offline_keeps_the_loopback() {
        let env = env();
        let sh = super::linux(&env, "reader", &Step::Egress).expect("the step renders");
        let lo = sh.find("oifname \"lo\" accept").expect("the loopback stays open");
        assert!(lo < sh.find("ct state new drop").expect("the drop"), "{sh}");
    }

    #[test]
    fn ready_leaves_its_marker() {
        let env = env();
        let sh = rendered(&env, "writer", |s| matches!(s, Step::Ready));
        assert!(sh.contains("/var/lib/isoloom/ready"), "{sh}");
    }

    #[test]
    fn windows_has_hosts_but_no_trunks() {
        let env = env();
        let hosts = env.machines["reader"]
            .lifecycle
            .iter()
            .find(|s| matches!(s, Step::Hosts { .. }))
            .expect("planned");
        assert!(super::windows(&env, "reader", hosts).is_some_and(|ps| ps.contains("writer store")));
        assert!(super::windows(&env, "reader", &Step::Trunks).is_none());
    }

    #[test]
    fn netmask_of_a_block() {
        assert_eq!(super::mask_of("10.1.0.0/24").to_string(), "255.255.255.0");
        assert_eq!(super::network_of("10.1.0.0/24"), "10.1.0.0");
    }
}
