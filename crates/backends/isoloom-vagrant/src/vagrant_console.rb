# The VMs' graphical consoles, on this computer's loopback only.
# - VMware: the port and password the runner chose for a machine, from ISOLOOM_CONSOLES
#   ({"<machine>": {"port": <n>, "password": "<8 characters>"}}); no console when it chose none.
# - QEMU: a display, a USB tablet, and VNC at the first free display from 5900.
# - VirtualBox: its remote display only with the Oracle extension pack, which brings the server.
module IsoloomConsole
  # The console the runner chose for `machine`, or nil.
  def self.of(machine)
    text = ENV["ISOLOOM_CONSOLES"].to_s
    return nil if text.empty?
    require "json"
    console = JSON.parse(text)[machine]
    console.is_a?(Hash) && console["port"].is_a?(Integer) && console["password"].is_a?(String) ? console : nil
  rescue JSON::ParserError
    nil
  end

  # QEMU's arguments for a VM of CPU `arch`: an Arm VM has no display unless given one.
  def self.qemu(arch)
    usb = ["-device", "qemu-xhci,id=isoloom-usb", "-device", "usb-tablet,bus=isoloom-usb.0"]
    display = arch == "aarch64" ? ["-device", "virtio-gpu-pci", "-device", "usb-kbd,bus=isoloom-usb.0"] : []
    ["-vnc", "127.0.0.1:0,to=99"] + usb.first(2) + display + usb.last(2)
  end

  # Whether VirtualBox has a remote display server: an installed, usable extension pack that
  # brings one. Asked once per Vagrant command, and only of a VirtualBox VM's block.
  def self.virtualbox_vrde?
    return @vrde unless @vrde.nil?
    require "open3"
    dir = ENV["VBOX_MSI_INSTALL_PATH"] || ENV["VBOX_INSTALL_PATH"]
    tool = dir && File.exist?(File.join(dir, "VBoxManage.exe")) ? File.join(dir, "VBoxManage.exe") : "VBoxManage"
    out, status = Open3.capture2e(tool, "list", "extpacks")
    packs = out.split(/^Pack no\.\s*\d+:/).drop(1)
    @vrde = status.success? && packs.any? { |p| p =~ /^Usable:\s+true/ && p =~ /^VRDE Module:\s+\S/ }
  rescue SystemCallError
    @vrde = false
  end
end
