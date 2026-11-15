# UTM (`vagrant up --provider utm`, vagrant_utm 0.2.0 or later, UTM from GitHub or Homebrew: the
# App Store build has no vmnet). The plugin makes no adapter for a private network and sets no
# address in the guest, so Isoloom does it:
# - each network is an isolated vmnet network (QEMU's `vmnet-host` with a `net-uuid`): the VMs
#   on it see each other and nothing else, not the host, not another network, and get no DHCP.
#   Its id is made from this copy's folder and the network's name (`net_uuid`): two copies of an
#   environment never share a network, and a reload finds its own again;
# - before each boot (the VM stopped, its configuration editable), the VM's adapters on them, with
#   the MACs Isoloom gives them, as QEMU arguments of its UTM configuration, the earlier ones of
#   Isoloom's replaced and the plugin's own (its shared folders) kept (`attach`);
# - once the guest answers, before its provisioning, each adapter's address set in the guest, found
#   by its MAC (`address`), as on Hyper-V.
# Every call to UTM is an Apple event: UTM must be open in this session, and this app allowed to
# control it (System Settings, Privacy & Security, Automation).
module IsoloomUtm
  def self.fail!(message)
    raise Class.new(Vagrant::Errors::VagrantError) { define_method(:error_message) { "UTM: #{message}" } }
  end

  # Network `net`'s vmnet id: a UUID (version 4 layout) made from this copy's folder and the
  # network's name.
  def self.net_uuid(net)
    require "digest"
    h = Digest::SHA256.hexdigest("#{__dir__}\0#{net}")
    variant = ((h[16].to_i(16) & 0x3) | 0x8).to_s(16)
    "#{h[0, 8]}-#{h[8, 4]}-4#{h[13, 3]}-#{variant}#{h[17, 3]}-#{h[20, 12]}"
  end

  # The marker of the QEMU arguments Isoloom puts in a VM's UTM configuration: its netdevs' ids.
  MARK = "isoloomnet"

  # Replaces the VM's QEMU arguments marked `MARK` with `args`, the others kept (AppleScript, read
  # from stdin; the VM is stopped).
  SET_ARGS = <<~'APPLESCRIPT'
    on run argv
      set vmId to item 1 of argv
      set mark to item 2 of argv
      tell application "UTM"
        set vm to virtual machine id vmId
        set config to configuration of vm
        set kept to {}
        repeat with arg in (qemu additional arguments of config)
          if (argument string of arg) does not contain mark then set end of kept to (contents of arg)
        end repeat
        repeat with i from 3 to (count of argv)
          set end of kept to {argument string:(item i of argv)}
        end repeat
        set qemu additional arguments of config to kept
        update configuration of vm with config
      end tell
    end run
  APPLESCRIPT

  # Before the VM boots: an adapter on each network of `nics` (`[[network, mac], ...]`), in
  # order, a virtio card on the network's vmnet network.
  def self.attach(machine, nics)
    require "open3"
    args = nics.each_with_index.flat_map do |(net, mac), i|
      ["-netdev vmnet-host,id=#{MARK}#{i},net-uuid=#{net_uuid(net)}",
       "-device virtio-net-pci,netdev=#{MARK}#{i},mac=#{mac}"]
    end
    out, status = Open3.capture2e("osascript", "-", machine.id.to_s, MARK, *args, stdin_data: SET_ARGS)
    fail!("#{machine.name}'s adapters on the environment's networks couldn't be set: #{out.strip}") unless status.success?
  end

  # Once the guest answers (a UTM step, so this runs on UTM only), before its provisioning: its
  # addresses on the networks.
  def self.address(machine, script)
    fail!("#{machine.name} didn't answer within #{machine.config.vm.boot_timeout} s") unless machine.communicate.wait_for_ready(machine.config.vm.boot_timeout)
    machine.communicate.sudo(script)
  end
end
