# Hyper-V (`vagrant up --provider hyperv`, from an elevated shell: an administrator, or a member
# of Hyper-V Administrators). Vagrant connects every adapter of a VM to one virtual switch and
# sets no address in the guest, so Isoloom does the rest:
# - the first adapter (Vagrant's own connection, and the internet) is on ISOLOOM_HYPERV_SWITCH,
#   else the Default Switch (Windows 10 and 11; on Windows Server, name an external or NAT one);
# - each network is a private switch named after it (between the VMs only, like a VirtualBox
#   internal network), created when missing and removed with the environment's last VM;
# - each VM has an adapter there with a fixed MAC, attached before every start (Vagrant has
#   just connected them all to the first switch), and its address set in the guest once it
#   answers (a boot-time unit on Linux, a static address on Windows);
# - published ports aren't forwarded to the host (Hyper-V can't): Windows' port proxy relays each
#   one (TCP) from 127.0.0.1 to the VM's address, set at every start (the address may change)
#   and removed with the VM.
HYPERV_SWITCH = ENV.fetch("ISOLOOM_HYPERV_SWITCH", "Default Switch")
module IsoloomHyperV
  def self.fail!(message)
    raise Class.new(Vagrant::Errors::VagrantError) { define_method(:error_message) { "Hyper-V: #{message}" } }
  end

  # Runs PowerShell on this host: its output, or the error it stopped on.
  def self.ps(script)
    require "vagrant/util/powershell"
    r = Vagrant::Util::PowerShell.execute_inline("$ErrorActionPreference = 'Stop'; #{script}")
    fail!(r.stderr.strip.empty? ? r.stdout.strip : r.stderr.strip) if r.exit_code != 0
    r.stdout
  end

  # A PowerShell string.
  def self.q(s)
    "'#{s.to_s.gsub("'", "''")}'"
  end

  # Before Vagrant picks the first adapter's switch: that switch exists (Vagrant would otherwise
  # ask which one, or take another), and so do the networks' private switches.
  def self.switches(names)
    ps((["if (-not (Hyper-V\\Get-VMSwitch -Name #{q(HYPERV_SWITCH)} -ErrorAction SilentlyContinue)) { throw ('no virtual switch named ' + #{q(HYPERV_SWITCH)} + ': set ISOLOOM_HYPERV_SWITCH to one of Get-VMSwitch') }"] +
        names.map { |n| "if (-not (Hyper-V\\Get-VMSwitch -Name #{q(n)} -ErrorAction SilentlyContinue)) { Hyper-V\\New-VMSwitch -Name #{q(n)} -SwitchType Private | Out-Null }" }).join("; "))
  end

  # Before the VM starts: its adapter on each network's switch, found by its MAC (added the
  # first time), connected there.
  def self.attach(machine, nics)
    return if nics.empty?
    ps((["$vm = Hyper-V\\Get-VM -Id #{q(machine.id)}"] + nics.map { |sw, mac|
      "$a = @(Hyper-V\\Get-VMNetworkAdapter -VM $vm | Where-Object { $_.MacAddress -eq #{q(mac)} }); " \
      "if ($a.Count -eq 0) { Hyper-V\\Add-VMNetworkAdapter -VM $vm -Name #{q(sw)} -SwitchName #{q(sw)} -StaticMacAddress #{q(mac)} } " \
      "else { Hyper-V\\Connect-VMNetworkAdapter -VMNetworkAdapter $a[0] -SwitchName #{q(sw)} }"
    }).join("; "))
  end

  # Before the VM starts: a virtual TPM 2.0, with a key protector of this host's own (Hyper-V
  # enables none without one). A generation 1 VM has no TPM to give: an error when the spec
  # requires one, else it starts without (Windows 11's boxes install without one).
  def self.tpm(machine, required)
    ps("$vm = Hyper-V\\Get-VM -Id #{q(machine.id)}; " \
       "if ($vm.Generation -lt 2) { if (#{required ? '$true' : '$false'}) { throw ($vm.Name + ' is a generation 1 VM: it has no virtual TPM') } } " \
       "elseif (-not (Hyper-V\\Get-VMSecurity -VM $vm).TpmEnabled) { Hyper-V\\Set-VMKeyProtector -VM $vm -NewLocalKeyProtector; Hyper-V\\Enable-VMTPM -VM $vm }")
  end

  # Once the VM has an address (a Hyper-V step, so this runs on Hyper-V only), when the guest
  # answers, before its provisioning: its addresses on the networks.
  def self.address(machine, script)
    fail!("#{machine.name} didn't answer within #{machine.config.vm.boot_timeout} s") unless machine.communicate.wait_for_ready(machine.config.vm.boot_timeout)
    machine.communicate.sudo(script)
  end

  # Hyper-V forwards no ports: each published one (guest => host) relayed from this host's
  # 127.0.0.1 to the VM's address by Windows' port proxy (TCP; the IP Helper service), set again
  # at every start since the address may change.
  def self.ports(machine, ports)
    ip = (machine.ssh_info || {})[:host]
    fail!("#{machine.name} has no address to relay its ports to") unless ip
    ps(ports.map { |guest, host|
      "netsh interface portproxy delete v4tov4 listenaddress=127.0.0.1 listenport=#{host} | Out-Null; " \
      "netsh interface portproxy add v4tov4 listenaddress=127.0.0.1 listenport=#{host} connectaddress=#{ip} connectport=#{guest} | Out-Null; " \
      "if ($LASTEXITCODE -ne 0) { throw 'netsh interface portproxy add failed for 127.0.0.1:#{host}' }"
    }.join("; "))
    machine.ui.info("Hyper-V: " + ports.map { |guest, host| "127.0.0.1:#{host} relayed to #{ip}:#{guest}" }.join("; "))
  end

  # After a VM is deleted: its ports' relays.
  def self.unports(ports)
    ps(ports.map { |_, host| "netsh interface portproxy delete v4tov4 listenaddress=127.0.0.1 listenport=#{host} | Out-Null" }.join("; ") + "; $global:LASTEXITCODE = 0")
  end

  # After a VM is deleted: the networks' switches no VM uses any more.
  def self.prune(names)
    ps(names.map { |n| "if ((Hyper-V\\Get-VMSwitch -Name #{q(n)} -ErrorAction SilentlyContinue) -and -not (Hyper-V\\Get-VMNetworkAdapter -All | Where-Object { $_.SwitchName -eq #{q(n)} })) { Hyper-V\\Remove-VMSwitch -Name #{q(n)} -Force }" }.join("; "))
  end
end
