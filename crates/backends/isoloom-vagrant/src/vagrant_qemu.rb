# QEMU (vagrant-qemu): what the provider block can't say on its own.
# - Every network gets a hub: one QEMU process with no machine, a port per VM on one emulated
#   hub, each VM connecting to its own (no root; multicast doesn't send on a Mac). The ports are
#   picked when Vagrant reads this file (`ports`), for this copy of the project: free on this
#   host, and the ones a running hub of this copy listens on while it runs. A hub starts before a
#   VM on its network does, unless one runs, with `SPARE` ports more for VMs that join the network
#   while it runs; it is never restarted under the VMs connected to it, and stops once the
#   network's last VM is destroyed: destroying one VM leaves it to the others.
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

  # The hub's record: the ports it listens on, by VM, and its spare ones (`record`).
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
  # The ports a hub listens on beyond its VMs', for VMs that join its network while it runs (an
  # attack box added to a running environment): QEMU's socket netdev doesn't reconnect, so a hub
  # restarted for one more port would leave the VMs connected to it off the network for good.
  SPARE = 4

  # Each network's hub ports, by VM (`vms`: each network's VMs, in the order they start), as
  # QEMU_HUBS holds them. A network whose hub runs keeps the ports it listens on, a VM that joins
  # it taking a spare one (`kept`). Every other VM gets a port free on this host, looked for from a
  # point of this folder's, not of the environment's name: two copies of one environment start far
  # apart, and a port something else holds is passed over. One folder starts from the same point,
  # so a copy gets the same ports from one run to the next while they stay free.
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

  # The network's running hub's ports for its VMs `names`: each VM's own, and a spare one, in
  # order, for a VM that joins the network while the hub runs. None when no hub runs, or when its
  # spare ports don't go round the VMs that join.
  def self.kept(net, names)
    rec = record(net)
    return nil unless rec
    spare = rec[:spare].dup
    ports = {}
    names.each do |vm|
      port = rec[:vms][vm] || spare.shift
      return nil unless port
      ports[vm] = port
    end
    ports
  end

  # The running hub's record: its ports by VM (`vms`) and those no VM has taken yet (`spare`).
  # None while no hub runs, or for a record Isoloom doesn't read; one an earlier Isoloom wrote
  # (the ports by VM alone) has no spare port.
  def self.record(net)
    return nil unless running(net)
    require "json"
    rec = JSON.parse(File.read(ports_file(net)))
    return nil unless rec.is_a?(Hash)
    vms, spare = rec["vms"].is_a?(Hash) ? [rec["vms"], rec["spare"]] : [rec, []]
    return nil unless vms.values.all?(Integer) && spare.is_a?(Array) && spare.all?(Integer)
    { vms: vms, spare: spare }
  rescue SystemCallError, JSON::ParserError
    nil
  end

  def self.write_record(net, vms, spare)
    require "json"
    File.write(ports_file(net), JSON.generate("vms" => vms, "spare" => spare))
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

  # Starts the network's hub on `ports` (by VM), with `SPARE` ports more, unless one runs: QEMU
  # returns once its ports listen. A running hub is kept: a VM that joins its network takes one of
  # its spare ports (`kept`), recorded here. One with no spare port left for it is refused rather
  # than restarted under the VMs connected to it. A hub without a record Isoloom reads (an older
  # Isoloom started it) is replaced: the VM about to start connects to its new port.
  def self.hub(net, ports)
    rec = record(net)
    if rec
      joined = ports.reject { |vm, port| rec[:vms][vm] == port }
      unless joined.values.all? { |port| rec[:spare].include?(port) }
        fail!("the hub of network `#{net}` runs for #{rec[:vms].keys.join(', ')}, with no spare port left " \
              "for #{joined.keys.join(', ')}: started again it would leave them off the network. " \
              "Destroy the environment, then bring it up again")
      end
      write_record(net, rec[:vms].merge(joined), rec[:spare] - joined.values) unless joined.empty?
      return
    end
    stop(net)
    require "fileutils"
    FileUtils.mkdir_p(hubs_dir)
    taken = ports.values.dup
    at = taken.max || start
    spare = Array.new(SPARE) do
      at = free_from(at, taken)
      taken << at
      at
    end
    args = ["qemu-system-#{HOST_ARCH}", "-machine", "none", "-display", "none", "-nodefaults",
            "-daemonize", "-pidfile", pid_file(net)]
    (ports.values + spare).each_with_index do |port, i|
      args += ["-netdev", "socket,id=s#{i},listen=127.0.0.1:#{port}", "-netdev", "hubport,id=h#{i},hubid=0,netdev=s#{i}"]
    end
    log = File.join(hubs_dir, "#{net}.log")
    ok = system(*args, out: log, err: log)
    fail!("the hub of network `#{net}` didn't start (#{args.first}): see #{log}") unless ok
    write_record(net, ports, spare)
  end

  # Stops the network's hub once none of its VMs (`vms`, by define name, and those its record
  # holds) is left besides the one just destroyed. A VM is left while Vagrant keeps its id: a
  # halted one still holds its port, and the hub starts again for it anyway if stopped.
  def self.release(net, vms, env, gone)
    rec = record(net)
    left = (vms + (rec ? rec[:vms].keys : [])).uniq.reject { |vm| vm == gone.to_s }
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

  # The guest account and group vagrant-qemu maps to this computer's user in a VirtioFS share
  # (`virtiofs_guest_uid` and `_gid`, its default, set so).
  GUEST_ID = 1000
  # The ids a squash covers end below u32::MAX, -1 to chown ("no change"), which no one owns.
  ID_END = 4_294_967_295

  # virtiofsd's arguments for a host folder (`host_folders`) every account of the VM writes, as
  # on the other providers: vagrant-qemu maps only its guest user and group to this computer's
  # (`--translate-uid map:1000:<uid>:1`), and an unprivileged virtiofsd refuses any other id
  # (EPERM: the attack box's player, root in docker-vm's VM), whatever the folder's permissions
  # say. Every other guest uid and gid is squashed to this computer's user and group, in two
  # ranges either side of the mapped one, so no two rules overlap. Only where the virtiofsd
  # vagrant-qemu starts translates ids (1.13 or later, `--translate-uid` in its `--help`, as
  # vagrant-qemu checks before its own map): an older one would refuse the arguments.
  def self.virtiofs_squash
    return [] unless virtiofsd_translates?
    [["--translate-uid", Process.uid], ["--translate-gid", Process.gid]].flat_map do |flag, host|
      [flag, "squash-guest:0:#{host}:#{GUEST_ID}", flag, "squash-guest:#{GUEST_ID + 1}:#{host}:#{ID_END - GUEST_ID - 1}"]
    end
  end

  # Whether the virtiofsd vagrant-qemu would start translates ids: the one it finds (on the PATH,
  # then /usr/libexec and /usr/lib/qemu), its `--help` naming `--translate-uid`.
  def self.virtiofsd_translates?
    return @translates unless @translates.nil?
    dirs = ENV.fetch("PATH", "").split(File::PATH_SEPARATOR)
    found = (dirs.map { |d| File.join(d, "virtiofsd") } + %w[/usr/libexec/virtiofsd /usr/lib/qemu/virtiofsd])
            .find { |f| File.file?(f) && File.executable?(f) }
    @translates = !found.nil? && IO.popen([found, "--help"], err: %i[child out], &:read).include?("--translate-uid")
  rescue SystemCallError
    @translates = false
  end
end
