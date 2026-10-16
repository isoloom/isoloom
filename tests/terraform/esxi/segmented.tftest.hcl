# examples/segmented on ESXi: three networks, each a port group on a vSwitch of its own with no
# uplink, joined by Isoloom's router (a VM on the uplink port group and on every network at .254),
# which forwards with the `reach` rules (access to front on every port, front to back on 6379)
# and to the internet; `back` has no internet. ESXi has no firewall of its own in front of a VM:
# the separate vSwitches and the router are what keep the networks apart. Every run plans against
# mocked providers; the router's uplink address (what VMware Tools reports) is pinned below.

mock_provider "esxi" {
  override_during = plan
}

mock_provider "tls" {
  override_during = plan
}

mock_provider "local" {
  override_during = plan
}

override_resource {
  target          = esxi_guest.isoloom_router
  override_during = plan
  values          = { ip_address = "192.0.2.60" }
}

variables {
  esxi_hostname        = "esxi.test"
  esxi_password        = "not-a-secret"
  images               = { "debian-12" = "https://images.test/debian-12.ova", "ubuntu-24.04" = "https://images.test/ubuntu-24.04.ova" }
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "each_network_is_a_port_group_on_a_vswitch_of_its_own_without_uplink" {
  command = plan

  assert {
    condition     = [esxi_vswitch.front.name, esxi_vswitch.back.name, esxi_vswitch.access.name] == ["iso1-front", "iso1-back", "iso1-access"]
    error_message = "front, back and access must each have their own vSwitch, named after the slot"
  }
  assert {
    condition     = length(esxi_vswitch.front.uplink) == 0 && length(esxi_vswitch.back.uplink) == 0 && length(esxi_vswitch.access.uplink) == 0
    error_message = "no network's vSwitch may have an uplink: the router is its only way out"
  }
  assert {
    condition     = [esxi_portgroup.front.vswitch, esxi_portgroup.back.vswitch, esxi_portgroup.access.vswitch] == ["iso1-front", "iso1-back", "iso1-access"] && [esxi_portgroup.front.name, esxi_portgroup.back.name, esxi_portgroup.access.name] == ["iso1-front", "iso1-back", "iso1-access"]
    error_message = "each network's port group must be on its own vSwitch"
  }
}

run "the_router_stands_on_the_uplink_and_on_every_network_at_254" {
  command = plan

  assert {
    condition     = [for n in esxi_guest.isoloom_router.network_interfaces : "${n.virtual_network} ${n.mac_address}"] == ["VM Network 00:50:56:01:00:00", "iso1-front 00:50:56:01:00:01", "iso1-back 00:50:56:01:00:02", "iso1-access 00:50:56:01:00:03"]
    error_message = "the router's first interface must be on uplink_port_group, then one on each network, each with its slot's MAC"
  }
  assert {
    condition = yamldecode(base64decode(esxi_guest.isoloom_router.guestinfo["metadata"])).network.ethernets == {
      nic0 = { match = { macaddress = "00:50:56:01:00:00" }, dhcp4 = true }
      nic1 = { match = { macaddress = "00:50:56:01:00:01" }, addresses = ["10.61.10.254/24"] }
      nic2 = { match = { macaddress = "00:50:56:01:00:02" }, addresses = ["10.61.20.254/24"] }
      nic3 = { match = { macaddress = "00:50:56:01:00:03" }, addresses = ["10.61.99.254/24"] }
    }
    error_message = "the router must take its uplink by DHCP and .254 on each network, each on the interface with its MAC"
  }
}

run "the_router_forwards_the_reach_rules_and_nothing_else_between_networks" {
  command = plan

  assert {
    condition = split("\n", one([for f in yamldecode(trimprefix(base64decode(esxi_guest.isoloom_router.guestinfo["userdata"]), "#cloud-config\n")).write_files : f.content if f.path == "/etc/nftables.conf"])) == tolist([
      "flush ruleset",
      "table inet isoloom {",
      "  chain forward {",
      "    type filter hook forward priority 0; policy drop;",
      "    ct state established,related accept",
      "    ip saddr 10.61.99.0/24 ip daddr 10.61.10.0/24 accept",
      "    ip saddr 10.61.10.0/24 ip daddr 10.61.20.0/24 meta l4proto { tcp, udp } th dport { 6379 } accept",
      "    ip saddr 10.61.10.0/24 ip daddr 10.61.20.0/24 icmp type echo-request accept",
      "    ip saddr { 10.61.10.0/24, 10.61.20.0/24, 10.61.99.0/24 } ip daddr != { 10.61.10.0/24, 10.61.20.0/24, 10.61.99.0/24 } accept",
      "  }",
      "  chain prerouting {",
      "    type nat hook prerouting priority -100;",
      "  }",
      "  chain postrouting {",
      "    type nat hook postrouting priority 100;",
      "    ip saddr { 10.61.10.0/24, 10.61.20.0/24, 10.61.99.0/24 } ip daddr != { 10.61.10.0/24, 10.61.20.0/24, 10.61.99.0/24 } masquerade",
      "  }",
      "}",
      "",
    ])
    error_message = "the router must drop what crosses networks but access to front (every port) and front to back (6379, and ping), forward the networks out, and publish nothing"
  }
}

run "each_machine_is_on_its_network_at_its_address_routed_by_the_router" {
  command = plan

  assert {
    condition     = [for n in esxi_guest.cache.network_interfaces : n.virtual_network] == ["iso1-back"] && yamldecode(base64decode(esxi_guest.cache.guestinfo["metadata"])).network.ethernets.nic0.addresses == ["10.61.20.20/24"] && yamldecode(base64decode(esxi_guest.cache.guestinfo["metadata"])).network.ethernets.nic0.routes == [{ to = "0.0.0.0/0", via = "10.61.20.254" }]
    error_message = "cache must be on back at 10.61.20.20, through the router at 10.61.20.254"
  }
  assert {
    condition     = [for n in esxi_guest.web.network_interfaces : n.virtual_network] == ["iso1-front"] && yamldecode(base64decode(esxi_guest.web.guestinfo["metadata"])).network.ethernets.nic0.addresses == ["10.61.10.10/24"] && yamldecode(base64decode(esxi_guest.web.guestinfo["metadata"])).network.ethernets.nic0.routes == [{ to = "0.0.0.0/0", via = "10.61.10.254" }]
    error_message = "web must be on front at 10.61.10.10, through the router at 10.61.10.254"
  }
  assert {
    condition     = [for n in esxi_guest.user.network_interfaces : n.virtual_network] == ["iso1-access"] && yamldecode(base64decode(esxi_guest.user.guestinfo["metadata"])).network.ethernets.nic0.addresses == ["10.61.99.10/24"] && yamldecode(base64decode(esxi_guest.user.guestinfo["metadata"])).network.ethernets.nic0.routes == [{ to = "0.0.0.0/0", via = "10.61.99.254" }]
    error_message = "user must be on access at 10.61.99.10, through the router at 10.61.99.254"
  }
}

run "the_outputs_reach_the_machines_through_the_router" {
  command = plan

  assert {
    condition     = output.ip == "192.0.2.60"
    error_message = "`ip` must be the router's uplink address"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file the set-up writes"
  }
  assert {
    condition     = output.machines == { cache = "10.61.20.20", web = "10.61.10.10", user = "10.61.99.10" } && output.ssh_user == "isoloom"
    error_message = "`machines` must give each machine's lab address, reached as isoloom through the router"
  }
}

# The uplink fix (Phase 3): with no DHCP on the uplink, VMware Tools reports a lab address (or
# none); the apply fails with that reason instead of giving it as the lab's address.
run "a_router_reporting_a_lab_address_fails_the_apply" {
  command = plan

  override_resource {
    target          = esxi_guest.isoloom_router
    override_during = plan
    values          = { ip_address = "10.61.20.254" }
  }

  expect_failures = [esxi_guest.isoloom_router]
}

run "a_router_reporting_no_address_fails_the_apply" {
  command = plan

  override_resource {
    target          = esxi_guest.isoloom_router
    override_during = plan
    values          = { ip_address = "" }
  }

  expect_failures = [esxi_guest.isoloom_router]
}

# The slot names everything the environment puts on the host, so two environments in two slots
# share nothing: the vSwitches, the port groups, the VMs and their MACs.
run "another_slot_names_everything_apart" {
  command = plan

  variables {
    slot = 42
  }

  assert {
    condition     = [esxi_vswitch.front.name, esxi_portgroup.back.name, esxi_guest.isoloom_router.guest_name, esxi_guest.web.guest_name] == ["iso42-front", "iso42-back", "iso42-router", "iso42-web"]
    error_message = "slot 42's vSwitches, port groups and VMs must be named iso42-<name>"
  }
  assert {
    condition     = [for n in esxi_guest.isoloom_router.network_interfaces : n.mac_address] == ["00:50:56:2a:00:00", "00:50:56:2a:00:01", "00:50:56:2a:00:02", "00:50:56:2a:00:03"]
    error_message = "slot 42's router MACs must carry the slot (2a)"
  }
}

# VMware's static MACs are 00:50:56:00:00:00 to 00:50:56:3f:ff:ff: a slot above 63 is refused.
run "a_slot_beyond_vmware_s_static_macs_is_refused" {
  command = plan

  variables {
    slot = 64
  }

  expect_failures = [var.slot]
}

# The spec: `back` has `internet: false`, so its machines may not reach the internet. The router
# forwards and masquerades every network to the internet (machines install their software, and
# an offline machine is meant to block the internet itself once set up), so nothing outside cache
# stops it.
run "known_bug_the_router_forwards_a_network_without_internet_to_it" {
  command = plan

  assert {
    condition     = length([for l in split("\n", one([for f in yamldecode(trimprefix(base64decode(esxi_guest.isoloom_router.guestinfo["userdata"]), "#cloud-config\n")).write_files : f.content if f.path == "/etc/nftables.conf"])) : l if strcontains(l, "daddr != {") && strcontains(l, "10.61.20.0/24")]) == 0
    error_message = "the router must not forward nor masquerade back (internet: false) to the internet"
  }
}
