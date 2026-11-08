# Machines the runner adds to the running environment (its ad-hoc attack VM, `isothrum attach`),
# as GNS3 adds a node to a live topology: one Ruby file each under <state folder>/adhoc
# (VAGRANT_DOTFILE_PATH, else .vagrant), loaded at the end of `Vagrant.configure`, each calling
# IsoloomAdHoc.define. `vagrant up` never starts them (`autostart: false`): the runner boots one
# with `vagrant up <name> --provider=<p>` while the lab runs, and destroys it alone. The VM joins
# one lab network (ADHOC_NETWORKS), at an address the runner picked free, as the lab's own VMs
# join it on each provider; on QEMU through the port its network's hub keeps for it (QEMU_HUBS).
# Its box is the runner's (`box:`, else ISOLOOM_ACCESS_BOX), else this host's default: Kali on
# x86, the Debian cloud image on Arm (Kali's box has no arm64 build). With an `image:` the VM is
# a container run as a VM, as Isoloom runs a container machine on this target: Debian 12 with
# Docker (ADHOC_DOCKER), the image idle in it on the VM's own network, so the VM's lab address is
# the attacker's; the shell is then `docker exec` into it. Hyper-V, ESXi and UTM give a late VM
# no lab network this way (a spec's own `access: true` machine does run there).
require "shellwords"

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

  # The shell that runs `image` idle in the VM named `name`, on the VM's own network (its lab
  # address is the box's), with the capabilities an attacker's tools need, as the Compose access
  # service runs it; a box a previous run left goes first.
  def self.attack_box(name, image)
    n = Shellwords.escape(name)
    i = Shellwords.escape(image)
    "docker rm -f #{n} >/dev/null 2>&1 || true\ndocker pull #{i}\ndocker run -d --name #{n} --restart unless-stopped --network host --cap-add NET_ADMIN --cap-add NET_RAW --entrypoint sleep #{i} infinity"
  end

  # A VM `name` on lab network `network` at `ip`, defined (in `vagrant`, the configure block's
  # `config`) but not started: a VM of `box`, or one running the container `image` (then a Debian
  # 12 box with Docker, per provider, unless a box is named). `hostname` is the name it answers to
  # inside (the runner's attack box: `attackbox`; a container on the VM's network shares it), its
  # define name otherwise.
  def self.define(vagrant, name, network:, ip:, box: nil, image: nil, hostname: nil, cpus: 2, memory: 2048)
    net = ADHOC_NETWORKS.fetch(network) { fail!("no lab network `#{network}` (#{ADHOC_NETWORKS.keys.join(", ")})") }
    picked = box || ENV["ISOLOOM_ACCESS_BOX"]
    box = picked || (image ? ADHOC_IMAGE_BOX : ADHOC_BOX)
    # The Debian box of each provider the main one has no build for, when the image picks the box.
    debian = picked.nil? && image ? ADHOC_IMAGE_BOXES : {}
    label = "#{ADHOC_LABEL} · #{name}"
    vagrant.vm.define name, autostart: false do |m|
      m.vm.box = box
      m.vm.hostname = hostname || name
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
      m.vm.provider "libvirt" do |v, o|
        o.vm.box = debian["libvirt"] if debian["libvirt"]
        # The host's build of a multi-architecture box, as Isoloom's own VMs take it.
        o.vm.box_architecture = :auto
        v.cpus = cpus
        v.memory = memory
      end
      m.vm.provider "qemu" do |v, o|
        if debian["qemu"]
          o.vm.box = debian["qemu"]
          o.vm.box_version = ">= 0"
          # Room for the image (an attack box unpacks to 12 GB; the cloud image's disk is 10 GB): an
          # overlay of ADHOC_IMAGE_DISK_GB, sparse, so it costs what it holds.
          v.disk_resize = IsoloomQemu.disk(debian["qemu"], ADHOC_IMAGE_DISK_GB) if defined?(IsoloomQemu)
        end
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
      if image
        m.vm.provision "shell", name: "docker", inline: ADHOC_DOCKER
        m.vm.provision "shell", name: "attack box", inline: attack_box(name, image)
      end
    end
  end
end
