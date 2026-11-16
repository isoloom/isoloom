//! The names a Vagrantfile gives on the host: each carries the copy's key.

use std::path::Path;

use isoloom_test_support::needs;

use crate::testing::{Outcome, field, libvirt_networks, loadable, plan, printed, vagrantfile};

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
