//! What every Vagrantfile Isoloom writes says the same way, as [`ruby`](crate::ruby) pieces: the
//! inputs read at launch, the SSH proxy, libvirt's domain prefix, a published port, the
//! constraints' prelude and the Docker install. `var` is the block variable the statement is
//! for: `config` in a single-VM file, `m` inside a `config.vm.define`.

use isoloom_plan::{Environment, Step};
use isoloom_render::steps;
use isoloom_spec::model::Protocol;

use crate::ruby::{Call, Ruby, Value};

/// `<var>.vm.provision "shell", name: <name>`: a shell provisioner, its script still to add.
pub fn shell(var: &str, name: &str) -> Call {
    Call::new(format!("{var}.vm.provision")).arg(Value::str("shell")).kw("name", Value::str(name))
}

/// The values given at launch, read from the host's environment (empty when unset), as the
/// `INPUTS` hash provisioners pass on. Nothing when the environment has none.
pub fn inputs(r: &mut Ruby, inputs: &[String]) {
    if inputs.is_empty() {
        return;
    }
    r.comment("Values provided at launch (empty when unset).");
    let fetch = |i: &String| Value::raw(format!("ENV.fetch({}, \"\")", Value::str(i)));
    r.hash("INPUTS", inputs.iter().map(|i| (Value::str(i), fetch(i))));
}

/// A tool that can't reach the machines itself (macOS keeps third-party tools off the local
/// network) sets ISOLOOM_SSH_PROXY_COMMAND, e.g. "/usr/bin/nc %h %p", and Vagrant's SSH goes
/// through it. Unset: Vagrant connects directly.
pub fn ssh_proxy(r: &mut Ruby, var: &str) {
    let proxy = "ENV[\"ISOLOOM_SSH_PROXY_COMMAND\"]";
    r.assign_if(&format!("{var}.ssh.proxy_command"), Value::raw(proxy), proxy);
}

/// libvirt names a domain `<prefix><machine>`, the prefix defaulting to the folder's name: the
/// environment's name keeps two environments' domains apart.
pub fn libvirt_prefix(r: &mut Ruby, var: &str, env: &str) {
    r.block(Call::new(format!("{var}.vm.provider")).arg(Value::str("libvirt")), "v", |r| {
        r.assign("v.default_prefix", Value::str(&format!("{env}_")));
    });
}

/// A port forwarded from the host's loopback: `guest` in the VM to `host` on this machine. UDP
/// says so (vagrant-libvirt skips those, with a warning; `isoloom run` repeats it).
pub fn forwarded_port(var: &str, guest: u16, host: u16, protocol: Protocol) -> Call {
    let call = Call::new(format!("{var}.vm.network"))
        .arg(Value::str("forwarded_port"))
        .kw("guest", guest)
        .kw("host", host)
        .kw("host_ip", Value::str("127.0.0.1"));
    if protocol.is_tcp() {
        call
    } else {
        call.kw("protocol", Value::str("udp"))
            .comment("not on libvirt: vagrant-libvirt skips UDP forwards")
    }
}

/// The spec's constraints (proxy, mirrors, NTP) as the VM's first provisioner, rendered by the
/// plan's [`Step::Prelude`]. Nothing when the spec has none.
pub fn prelude(r: &mut Ruby, env: &Environment, var: &str) {
    if let Some(sh) = steps::linux(env, "", &Step::Prelude) {
        r.call(shell(var, "constraints").kw("inline", Value::str(&sh)));
    }
}

/// Docker from Docker's own install script, unless the box has it.
pub fn install_docker(r: &mut Ruby, var: &str) {
    let script = "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh";
    r.call(shell(var, "docker").kw("inline", Value::str(script)));
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Protocol;

    use super::{forwarded_port, inputs, ssh_proxy};
    use crate::ruby::Ruby;

    #[test]
    fn udp_forwards_say_libvirt_skips_them() {
        assert_eq!(
            forwarded_port("m", 53, 5353, Protocol::Udp).to_string(),
            "m.vm.network \"forwarded_port\", guest: 53, host: 5353, host_ip: \"127.0.0.1\", protocol: \"udp\" # not on libvirt: vagrant-libvirt skips UDP forwards"
        );
        assert_eq!(
            forwarded_port("config", 80, 8080, Protocol::Tcp).to_string(),
            "config.vm.network \"forwarded_port\", guest: 80, host: 8080, host_ip: \"127.0.0.1\""
        );
    }

    #[test]
    fn inputs_are_read_from_the_host() {
        let mut r = Ruby::new();
        inputs(&mut r, &[]);
        ssh_proxy(&mut r, "config");
        inputs(&mut r, &["TOKEN".into()]);
        assert_eq!(
            r.finish(),
            "config.ssh.proxy_command = ENV[\"ISOLOOM_SSH_PROXY_COMMAND\"] if ENV[\"ISOLOOM_SSH_PROXY_COMMAND\"]\n# Values provided at launch (empty when unset).\nINPUTS = {\n  \"TOKEN\" => ENV.fetch(\"TOKEN\", \"\"),\n}\n"
        );
    }
}
