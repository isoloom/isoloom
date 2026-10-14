# QEMU (vagrant-qemu): what the provider block can't say on its own.
# - A network of other than two VMs gets a hub: one QEMU process with no machine, a port per VM
#   on one emulated hub, each VM connecting to its own (no root; multicast doesn't send on a
#   Mac). It starts before a VM on that network does, and stops with the network's first VM,
#   which `vagrant destroy` destroys last.
# - A VM's disk is an overlay of its box's, and a size below the box's disk would cut its file
#   system off: `disk_resize` is set only when the machine's size is above the box's (every
#   installed version of it, read with qemu-img). Until the box is downloaded the size is unknown
#   and nothing is set; Vagrant reads this file again once it has the box, before it makes the
#   disk.
module IsoloomQemu
  # Beside Vagrant's own state for this Vagrantfile.
  def self.hubs_dir
    File.join(__dir__, ".vagrant", "isoloom-qemu-hubs")
  end

  def self.fail!(message)
    raise Class.new(Vagrant::Errors::VagrantError) { define_method(:error_message) { "QEMU: #{message}" } }
  end

  def self.pid_file(net)
    File.join(hubs_dir, "#{net}.pid")
  end

  # The hub's process id, while it runs.
  def self.running(net)
    pid = Integer(File.read(pid_file(net)).strip)
    Process.kill(0, pid)
    pid
  rescue ArgumentError, SystemCallError
    nil
  end

  # Starts the network's hub unless it runs: QEMU returns once its ports listen.
  def self.hub(net, ports)
    return if running(net)
    require "fileutils"
    FileUtils.mkdir_p(hubs_dir)
    args = ["qemu-system-#{HOST_ARCH}", "-machine", "none", "-display", "none", "-nodefaults",
            "-daemonize", "-pidfile", pid_file(net)]
    ports.each_with_index do |port, i|
      args += ["-netdev", "socket,id=s#{i},listen=127.0.0.1:#{port}", "-netdev", "hubport,id=h#{i},hubid=0,netdev=s#{i}"]
    end
    log = File.join(hubs_dir, "#{net}.log")
    ok = system(*args, out: log, err: log)
    fail!("the hub of network `#{net}` didn't start (#{args.first}): see #{log}") unless ok
  end

  # Stops the network's hub, if it runs.
  def self.stop(net)
    pid = running(net)
    Process.kill("TERM", pid) if pid
    File.delete(pid_file(net)) if File.exist?(pid_file(net))
  end

  # `disk_resize` for a VM of `box` sized `gb` GB: the size when it is above the box's disk,
  # else nothing (the box's disk is kept, never cut).
  def self.disk(box, gb)
    require "json"
    dir = File.join(Vagrant.user_data_path.to_s, "boxes", box.gsub("/", "-VAGRANTSLASH-"))
    images = Dir.glob(File.join(dir, "**", "libvirt", "**", "*.{img,qcow2}"))
    return nil if images.empty?
    sizes = images.map do |img|
      out = IO.popen(["qemu-img", "info", "--output=json", img], err: File::NULL, &:read)
      JSON.parse(out)["virtual-size"]
    rescue StandardError
      nil
    end
    # A box whose size can't be read keeps its own disk: a guess could cut it.
    return nil if sizes.include?(nil)
    gb * 1024**3 > sizes.max ? "#{gb}G" : nil
  end
end
