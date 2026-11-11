# QEMU (vagrant-qemu): what the provider block can't say on its own.
# - Every network gets a hub: one QEMU process with no machine, a port per VM on one emulated
#   hub, each VM connecting to its own (no root; multicast doesn't send on a Mac). The ports are
#   picked when Vagrant reads this file (`ports`), for this copy of the project: free on this
#   host, and the ones a running hub of this copy listens on while it runs. A hub starts before a
#   VM on its network does, unless it runs with those ports, and stops once the network's last
#   VM is destroyed: destroying one VM leaves it to the others.
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

  # The ports the network's hub listens on, by VM, written when it starts.
  def self.ports_file(net)
    File.join(hubs_dir, "#{net}.ports.json")
  end

  # The hub's process id, while it runs: a process whose command names the hub's pid file, so
  # an id the system has given another process since is never taken for (or stopped as) a hub.
  def self.running(net)
    pid = Integer(File.read(pid_file(net)).strip)
    command = IO.popen(["ps", "-ww", "-o", "command=", "-p", pid.to_s], err: File::NULL, &:read)
    pid if command.include?(pid_file(net))
  rescue ArgumentError, SystemCallError
    nil
  end

  # The hub ports: 20000-39999.
  FIRST_PORT = 20_000
  PORTS = 20_000

  # Each network's hub ports, by VM (`vms`: each network's VMs, in the order they start), as
  # QEMU_HUBS holds them. A network whose hub runs keeps the ports it listens on. Every other VM
  # gets a port free on this host, looked for from a point of this folder's, not of the
  # environment's name: two copies of one environment start far apart, and a port something else
  # holds is passed over. One folder starts from the same point, so a copy gets the same ports
  # from one run to the next while they stay free.
  def self.ports(vms)
    hubs = vms.map { |net, names| [net, kept(net, names)] }.to_h
    taken = hubs.values.compact.flat_map(&:values)
    at = start
    hubs.each_key do |net|
      next if hubs[net]
      hubs[net] = vms[net].map do |vm|
        at = free_from(at, taken)
        taken << at
        [vm, at]
      end.to_h
    end
    hubs
  end

  # The ports the network's hub listens on, by VM, when it runs for exactly the VMs `names`.
  def self.kept(net, names)
    return nil unless running(net)
    require "json"
    ports = JSON.parse(File.read(ports_file(net)))
    ports if ports.is_a?(Hash) && ports.keys == names
  rescue SystemCallError, JSON::ParserError
    nil
  end

  # This folder's first port to look from: FNV-1a of its path, in the range.
  def self.start
    hash = File.expand_path(__dir__).bytes.reduce(0x811c9dc5) { |h, b| ((h ^ b) * 0x01000193) & 0xffffffff }
    FIRST_PORT + hash % PORTS
  end

  # The first port from `at` on, wrapping in the range, that is free here and not in `taken`.
  def self.free_from(at, taken)
    PORTS.times do |i|
      port = FIRST_PORT + (at - FIRST_PORT + i) % PORTS
      return port if !taken.include?(port) && free?(port)
    end
    fail!("no port of #{FIRST_PORT}-#{FIRST_PORT + PORTS - 1} is free for a network's hub")
  end

  # Whether a hub could listen on 127.0.0.1:`port` here.
  def self.free?(port)
    require "socket"
    TCPServer.new("127.0.0.1", port).close
    true
  rescue SystemCallError
    false
  end

  # Starts the network's hub on `ports` (by VM) unless it runs on them: QEMU returns once its
  # ports listen. A hub of the network on other ones (its VMs have changed, or an older
  # Vagrantfile started it) is stopped first: the VM about to start connects to its new port.
  def self.hub(net, ports)
    return if kept(net, ports.keys) == ports
    stop(net)
    require "fileutils"
    require "json"
    FileUtils.mkdir_p(hubs_dir)
    args = ["qemu-system-#{HOST_ARCH}", "-machine", "none", "-display", "none", "-nodefaults",
            "-daemonize", "-pidfile", pid_file(net)]
    ports.values.each_with_index do |port, i|
      args += ["-netdev", "socket,id=s#{i},listen=127.0.0.1:#{port}", "-netdev", "hubport,id=h#{i},hubid=0,netdev=s#{i}"]
    end
    log = File.join(hubs_dir, "#{net}.log")
    ok = system(*args, out: log, err: log)
    fail!("the hub of network `#{net}` didn't start (#{args.first}): see #{log}") unless ok
    File.write(ports_file(net), JSON.generate(ports))
  end

  # Stops the network's hub once none of its VMs (`vms`, by define name) is left besides the
  # one just destroyed. A VM is left while Vagrant keeps its id: a halted one still holds its
  # port, and the hub starts again for it anyway if stopped.
  def self.release(net, vms, env, gone)
    left = vms.reject { |vm| vm == gone.to_s }
    return if left.any? { |vm| env.local_data_path.join("machines", vm, "qemu", "id").file? }
    stop(net)
  end

  # Stops the network's hub, if it runs.
  def self.stop(net)
    pid = running(net)
    Process.kill("TERM", pid) if pid
    [pid_file(net), ports_file(net)].each { |f| File.delete(f) if File.exist?(f) }
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
