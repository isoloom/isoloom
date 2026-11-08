# Machines the runner adds to the running environment (its ad-hoc attack VM, `isothrum attach`),
# as GNS3 adds a node to a live topology: one Ruby file each under <state folder>/adhoc
# (VAGRANT_DOTFILE_PATH, else .vagrant), loaded at the end of `Vagrant.configure`, each calling
# IsoloomAdHoc.define. `vagrant up` never starts them (`autostart: false`): the runner boots one
# with `vagrant up <name> --provider=<p>` while the lab runs, and destroys it alone. The VM joins
# one lab network (ADHOC_NETWORKS), at an address the runner picked free, as the lab's own VMs
# join it on each provider; on QEMU through the port its network's hub keeps for it (QEMU_HUBS).
# Its box is the runner's (`box:`, else ISOLOOM_ACCESS_BOX), else this host's default: Kali on
# x86, the Debian cloud image on Arm (Kali's box has no arm64 build). Hyper-V, ESXi and UTM give
# a late VM no lab network this way (a spec's own `access: true` machine does run there).
module IsoloomAdHoc
  def self.fail!(message)
    raise Class.new(Vagrant::Errors::VagrantError) { define_method(:error_message) { "ad-hoc machine: #{message}" } }
  end

  # Where the runner writes its definitions.
  def self.dir
    File.join(ENV.fetch("VAGRANT_DOTFILE_PATH", File.join(__dir__, ".vagrant")), "adhoc")
  end

  # The definitions to load, in name order.
  def self.files
    Dir.glob(File.join(dir, "*.rb")).sort
  end

  # A VM `name` on lab network `network` at `ip`, defined (in `vagrant`, the configure block's
  # `config`) but not started.
  def self.define(vagrant, name, network:, ip:, box: nil, cpus: 2, memory: 2048)
    net = ADHOC_NETWORKS.fetch(network) { fail!("no lab network `#{network}` (#{ADHOC_NETWORKS.keys.join(", ")})") }
    box ||= ENV.fetch("ISOLOOM_ACCESS_BOX", ADHOC_BOX)
    label = "#{ADHOC_LABEL} · #{name}"
    vagrant.vm.define name, autostart: false do |m|
      m.vm.box = box
      m.vm.hostname = name
      m.vm.network "private_network", ip: ip, netmask: net["netmask"], virtualbox__intnet: net["name"], libvirt__network_name: net["name"], libvirt__dhcp_enabled: false
      m.vm.provider "virtualbox" do |v|
        v.name = label
        v.linked_clone = true
        v.cpus = cpus
        v.memory = memory
      end
      m.vm.provider "vmware_desktop" do |v|
        v.vmx["displayName"] = label
        v.vmx["numvcpus"] = cpus.to_s
        v.vmx["memsize"] = memory.to_s
      end
      m.vm.provider "parallels" do |v|
        v.name = label
        v.linked_clone = true
        v.cpus = cpus
        v.memory = memory
      end
      m.vm.provider "libvirt" do |v|
        v.cpus = cpus
        v.memory = memory
      end
      m.vm.provider "qemu" do |v|
        v.smp = "cpus=#{cpus}"
        v.memory = "#{memory}M"
        v.ssh_auto_correct = true
        port = defined?(QEMU_HUBS) ? QEMU_HUBS.dig(network, name) : nil
        fail!("QEMU: network `#{network}`'s hub keeps no port for `#{name}`") unless port
        v.advanced_network = true
        v.net_mode = :socket
        v.socket_opts = "connect=127.0.0.1:#{port}"
      end
      m.vm.provision "shell", name: "hosts", inline: ADHOC_HOSTS unless ADHOC_HOSTS.empty?
    end
  end
end
