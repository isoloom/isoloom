# examples/segmented on Proxmox: three networks, each a VNet of the environment's SDN zone, joined
# by Isoloom's router (a VM on the uplink bridge and on every VNet at .254), which forwards with
# the `reach` rules (access to front on every port, front to back on 6379) and to the internet;
# `back` has no internet. Each machine also has the Proxmox firewall in front of it. Every run
# plans against mocked providers; the VM ids and the router's uplink address are pinned below.

mock_provider "proxmox" {
  override_during = plan
}

mock_provider "tls" {
  override_during = plan
}

# What the guest agent reports: loopback, the uplink's DHCP address, and the router's lab
# addresses.
override_resource {
  target          = proxmox_virtual_environment_vm.isoloom_router
  override_during = plan
  values          = { ipv4_addresses = [["127.0.0.1"], ["192.0.2.50"], ["10.61.10.254"], ["10.61.20.254"], ["10.61.99.254"]] }
}

override_resource {
  target          = proxmox_virtual_environment_vm.cache
  override_during = plan
  values          = { vm_id = 101 }
}

override_resource {
  target          = proxmox_virtual_environment_vm.web
  override_during = plan
  values          = { vm_id = 102 }
}

override_resource {
  target          = proxmox_virtual_environment_vm.user
  override_during = plan
  values          = { vm_id = 103 }
}

override_resource {
  target          = proxmox_virtual_environment_file.router
  override_during = plan
  values          = { id = "local:snippets/iso1-router.yaml" }
}

override_resource {
  target          = proxmox_virtual_environment_file.cache
  override_during = plan
  values          = { id = "local:snippets/iso1-cache.yaml" }
}

override_resource {
  target          = proxmox_virtual_environment_file.web
  override_during = plan
  values          = { id = "local:snippets/iso1-web.yaml" }
}

override_resource {
  target          = proxmox_virtual_environment_file.user
  override_during = plan
  values          = { id = "local:snippets/iso1-user.yaml" }
}

variables {
  proxmox_endpoint = "https://pve.test:8006/"
  node             = "pve-test"
  ssh_public_key   = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "each_network_is_a_vnet_of_the_environment_s_zone" {
  command = plan

  assert {
    condition     = proxmox_sdn_zone_simple.env.id == "iso1" && tolist(proxmox_sdn_zone_simple.env.nodes) == tolist(["pve-test"])
    error_message = "the zone must be the slot's (iso1), on the node given"
  }
  assert {
    condition     = [proxmox_sdn_vnet.front.id, proxmox_sdn_vnet.back.id, proxmox_sdn_vnet.access.id] == ["i1n0", "i1n1", "i1n2"] && toset([proxmox_sdn_vnet.front.zone, proxmox_sdn_vnet.back.zone, proxmox_sdn_vnet.access.zone]) == toset(["iso1"])
    error_message = "front, back and access must each be a VNet of the zone, named after the slot"
  }
  assert {
    condition = [
      tolist([for c in proxmox_virtual_environment_firewall_ipset.front.cidr : c.name]),
      tolist([for c in proxmox_virtual_environment_firewall_ipset.back.cidr : c.name]),
      tolist([for c in proxmox_virtual_environment_firewall_ipset.access.cidr : c.name]),
    ] == [tolist(["10.61.10.0/24"]), tolist(["10.61.20.0/24"]), tolist(["10.61.99.0/24"])]
    error_message = "each network's IP set must hold its exact range"
  }
}

# The destroy fix (Phase 3): the SDN deletions are applied by a finalizer every SDN object depends
# on. Its ordering is configuration (`depends_on`), not a planned value: this asserts the
# finalizer and the applier are planned (an applier has no attribute of its own to read).
run "the_sdn_is_applied_on_create_and_finalized_on_destroy" {
  command = plan

  assert {
    condition     = proxmox_sdn_applier.finalizer != null && proxmox_sdn_applier.env != null
    error_message = "the SDN must have its applier and its destroy-time finalizer"
  }
}

run "the_router_stands_on_the_uplink_and_on_every_network_at_254" {
  command = plan

  assert {
    condition     = [for d in proxmox_virtual_environment_vm.isoloom_router.network_device : "${d.bridge} ${d.mac_address}"] == ["vmbr0 02:15:01:00:00:00", "i1n0 02:15:01:01:00:00", "i1n1 02:15:01:01:01:00", "i1n2 02:15:01:01:02:00"]
    error_message = "the router's first interface must be on uplink_bridge, then one on each VNet, each with its slot's MAC"
  }
  assert {
    condition = sort([for f in yamldecode(trimprefix(proxmox_virtual_environment_file.router.source_raw[0].data, "#cloud-config\n")).write_files : replace(f.content, "\n", " ") if startswith(f.path, "/etc/systemd/network/")]) == sort([
      "[Match] MACAddress=02:15:01:00:00:00  [Network] DHCP=yes",
      "[Match] MACAddress=02:15:01:01:00:00  [Network] Address=10.61.10.254/24 ConfigureWithoutCarrier=yes",
      "[Match] MACAddress=02:15:01:01:01:00  [Network] Address=10.61.20.254/24 ConfigureWithoutCarrier=yes",
      "[Match] MACAddress=02:15:01:01:02:00  [Network] Address=10.61.99.254/24 ConfigureWithoutCarrier=yes",
    ])
    error_message = "the router must take its uplink by DHCP and .254 on each network, each on the interface with its MAC"
  }
  assert {
    condition     = proxmox_virtual_environment_vm.isoloom_router.initialization[0].user_data_file_id == proxmox_virtual_environment_file.router.id
    error_message = "the router must boot with its own cloud-init snippet"
  }
}

# The router fix (Phase 3): the agent's uplink address is awaited 8 minutes, an IPv4 one, and the
# VM is stopped (not shut down) on destroy.
run "the_router_s_uplink_address_is_awaited_8_minutes" {
  command = plan

  assert {
    condition     = proxmox_virtual_environment_vm.isoloom_router.agent[0].enabled && proxmox_virtual_environment_vm.isoloom_router.agent[0].timeout == "8m" && proxmox_virtual_environment_vm.isoloom_router.agent[0].wait_for_ip[0].ipv4
    error_message = "the router's guest agent must be awaited 8 minutes for an IPv4 address"
  }
  assert {
    condition     = proxmox_virtual_environment_vm.isoloom_router.stop_on_destroy
    error_message = "the router must be stopped on destroy, so a router without its agent never holds the destroy"
  }
  assert {
    condition     = output.ip == "192.0.2.50"
    error_message = "`ip` must be the router's uplink address, never loopback nor a lab address"
  }
}

# ... and an uplink without DHCP fails the apply with its reason instead of going on.
run "a_router_without_an_uplink_address_fails_the_apply" {
  command = plan

  override_resource {
    target          = proxmox_virtual_environment_vm.isoloom_router
    override_during = plan
    values          = { ipv4_addresses = [["127.0.0.1"], ["10.61.10.254"], ["10.61.20.254"], ["10.61.99.254"]] }
  }

  expect_failures = [proxmox_virtual_environment_vm.isoloom_router]
}

run "the_router_forwards_the_reach_rules_and_nothing_else_between_networks" {
  command = plan

  assert {
    condition = split("\n", one([for f in yamldecode(trimprefix(proxmox_virtual_environment_file.router.source_raw[0].data, "#cloud-config\n")).write_files : f.content if f.path == "/etc/nftables.conf"])) == tolist([
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

run "each_machine_is_on_its_vnet_at_its_address_routed_by_the_router" {
  command = plan

  assert {
    condition     = [for d in proxmox_virtual_environment_vm.cache.network_device : "${d.bridge} ${d.firewall}"] == ["i1n1 true"] && proxmox_virtual_environment_vm.cache.initialization[0].ip_config[0].ipv4[0].address == "10.61.20.20/24" && proxmox_virtual_environment_vm.cache.initialization[0].ip_config[0].ipv4[0].gateway == "10.61.20.254"
    error_message = "cache must be on back's VNet at 10.61.20.20, behind the firewall, through the router at 10.61.20.254"
  }
  assert {
    condition     = [for d in proxmox_virtual_environment_vm.web.network_device : "${d.bridge} ${d.firewall}"] == ["i1n0 true"] && proxmox_virtual_environment_vm.web.initialization[0].ip_config[0].ipv4[0].address == "10.61.10.10/24" && proxmox_virtual_environment_vm.web.initialization[0].ip_config[0].ipv4[0].gateway == "10.61.10.254"
    error_message = "web must be on front's VNet at 10.61.10.10, behind the firewall, through the router at 10.61.10.254"
  }
  assert {
    condition     = [for d in proxmox_virtual_environment_vm.user.network_device : "${d.bridge} ${d.firewall}"] == ["i1n2 true"] && proxmox_virtual_environment_vm.user.initialization[0].ip_config[0].ipv4[0].address == "10.61.99.10/24" && proxmox_virtual_environment_vm.user.initialization[0].ip_config[0].ipv4[0].gateway == "10.61.99.254"
    error_message = "user must be on access's VNet at 10.61.99.10, behind the firewall, through the router at 10.61.99.254"
  }
  assert {
    condition     = toset([proxmox_virtual_environment_vm.isoloom_router.node_name, proxmox_virtual_environment_vm.cache.node_name, proxmox_virtual_environment_vm.web.node_name, proxmox_virtual_environment_vm.user.node_name]) == toset(["pve-test"])
    error_message = "every VM must go on the node given (no default node)"
  }
}

run "each_machine_s_firewall_admits_its_network_and_its_reach_rules_only" {
  command = plan

  assert {
    condition = alltrue([for o in [proxmox_virtual_environment_firewall_options.cache, proxmox_virtual_environment_firewall_options.web, proxmox_virtual_environment_firewall_options.user] : o.enabled && o.input_policy == "DROP" && o.output_policy == "ACCEPT"]) && [proxmox_virtual_environment_firewall_options.cache.vm_id, proxmox_virtual_environment_firewall_options.web.vm_id, proxmox_virtual_environment_firewall_options.user.vm_id] == [101, 102, 103]
    error_message = "each machine's firewall must be on, dropping what comes in unless a rule accepts it"
  }
  assert {
    condition = proxmox_virtual_environment_firewall_rules.cache.vm_id == 101 && sort([for r in proxmox_virtual_environment_firewall_rules.cache.rule : "${r.type} ${r.action} ${coalesce(r.source, "-")} ${coalesce(r.dest, "-")} ${coalesce(r.proto, "-")} ${coalesce(r.dport, "-")} ${coalesce(r.macro, "-")}"]) == sort([
      "in ACCEPT +iso1n1 - - - -",
      "in ACCEPT +iso1n0 +iso1n1 tcp 6379 -",
      "in ACCEPT +iso1n0 +iso1n1 udp 6379 -",
      "in ACCEPT +iso1n0 +iso1n1 - - Ping",
    ])
    error_message = "cache (on back) must admit back, and front on 6379 and ping (reach), and nothing else"
  }
  assert {
    condition = proxmox_virtual_environment_firewall_rules.web.vm_id == 102 && sort([for r in proxmox_virtual_environment_firewall_rules.web.rule : "${r.type} ${r.action} ${coalesce(r.source, "-")} ${coalesce(r.dest, "-")} ${coalesce(r.proto, "-")} ${coalesce(r.dport, "-")} ${coalesce(r.macro, "-")}"]) == sort([
      "in ACCEPT +iso1n0 - - - -",
      "in ACCEPT +iso1n2 +iso1n0 - - -",
    ])
    error_message = "web (on front) must admit front, and access on every port (reach), and nothing else"
  }
  assert {
    condition = proxmox_virtual_environment_firewall_rules.user.vm_id == 103 && sort([for r in proxmox_virtual_environment_firewall_rules.user.rule : "${r.type} ${r.action} ${coalesce(r.source, "-")} ${coalesce(r.dest, "-")} ${coalesce(r.proto, "-")} ${coalesce(r.dport, "-")} ${coalesce(r.macro, "-")}"]) == sort([
      "in ACCEPT +iso1n2 - - - -",
    ])
    error_message = "user (on access) must admit access only: no reach rule leads into access"
  }
}

# isoloom-ready/1: each step writes `running: <step>` first, `failed: <step>` and stops when it
# fails, and the set-up ends with `ready`.
run "each_machine_reports_its_set_up_in_the_ready_file" {
  command = plan

  assert {
    condition     = [for l in slice(yamldecode(trimprefix(proxmox_virtual_environment_file.cache.source_raw[0].data, "#cloud-config\n")).runcmd, 1, 8) : regex("^echo 'running: ([^']*)' > /var/lib/isoloom/ready; ", l)[0]] == ["growing the disk", "hosts file", "routes", "volume folders", "provision/cache.sh", "closing the internet", "health check"]
    error_message = "cache's steps must each say `running: <step>` in the ready file first, in order"
  }
  assert {
    condition     = alltrue([for l in slice(yamldecode(trimprefix(proxmox_virtual_environment_file.cache.source_raw[0].data, "#cloud-config\n")).runcmd, 1, 8) : regex("^echo 'running: ([^']*)'", l)[0] == regex("\\|\\| \\{ echo 'failed: ([^']*)' > /var/lib/isoloom/ready; exit 1; \\}$", l)[0]])
    error_message = "each of cache's steps must write `failed: <step>` and stop the set-up when it fails"
  }
  assert {
    condition     = endswith(element(yamldecode(trimprefix(proxmox_virtual_environment_file.cache.source_raw[0].data, "#cloud-config\n")).runcmd, 7), "echo ready > /var/lib/isoloom/ready' || { echo 'failed: health check' > /var/lib/isoloom/ready; exit 1; }")
    error_message = "cache's set-up must end by writing `ready`"
  }
  assert {
    condition     = [for l in slice(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 1, 6) : regex("^echo 'running: ([^']*)' > /var/lib/isoloom/ready; ", l)[0]] == ["hosts file", "routes", "waiting for cache", "provision/web.sh", "health check"]
    error_message = "web's steps must each say `running: <step>` first, waiting for cache before its own"
  }
  assert {
    condition     = [for l in slice(yamldecode(trimprefix(proxmox_virtual_environment_file.user.source_raw[0].data, "#cloud-config\n")).runcmd, 1, 4) : regex("^echo 'running: ([^']*)' > /var/lib/isoloom/ready; ", l)[0]] == ["hosts file", "routes", "health check"]
    error_message = "user's steps must each say `running: <step>` first"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the file the steps write"
  }
}

run "the_outputs_reach_the_machines_through_the_router" {
  command = plan

  assert {
    condition     = output.machines == { cache = "10.61.20.20", web = "10.61.10.10", user = "10.61.99.10" } && output.ssh_user == "isoloom"
    error_message = "`machines` must give each machine's lab address, reached as isoloom through the router"
  }
}

# The slot names everything the environment puts on the server, so two environments in two slots
# share nothing: the zone, the VNets, the pool, the VMs, their snippets, the image file, the IP
# sets and the router's MACs.
run "another_slot_names_everything_apart" {
  command = plan

  variables {
    slot = 42
  }

  assert {
    condition     = proxmox_sdn_zone_simple.env.id == "iso42" && [proxmox_sdn_vnet.front.id, proxmox_sdn_vnet.back.id, proxmox_sdn_vnet.access.id] == ["i42n0", "i42n1", "i42n2"] && proxmox_virtual_environment_pool.env.pool_id == "iso42"
    error_message = "slot 42's zone, VNets and pool must be iso42, i42n0..2"
  }
  assert {
    condition     = [proxmox_virtual_environment_vm.isoloom_router.name, proxmox_virtual_environment_vm.cache.name, proxmox_virtual_environment_vm.web.name, proxmox_virtual_environment_vm.user.name] == ["iso42-router", "iso42-cache", "iso42-web", "iso42-user"]
    error_message = "slot 42's VMs must be named iso42-<machine>"
  }
  assert {
    condition     = [proxmox_virtual_environment_file.router.source_raw[0].file_name, proxmox_virtual_environment_file.cache.source_raw[0].file_name, proxmox_download_file.debian_12.file_name] == ["iso42-router.yaml", "iso42-cache.yaml", "iso42-debian-12.img"]
    error_message = "slot 42's snippets and image file must be its own"
  }
  assert {
    condition     = [proxmox_virtual_environment_firewall_ipset.front.name, proxmox_virtual_environment_firewall_ipset.back.name, proxmox_virtual_environment_firewall_ipset.access.name] == ["iso42n0", "iso42n1", "iso42n2"]
    error_message = "slot 42's IP sets must be its own"
  }
  assert {
    condition     = [for d in proxmox_virtual_environment_vm.isoloom_router.network_device : d.mac_address] == ["02:15:2A:00:00:00", "02:15:2A:01:00:00", "02:15:2A:01:01:00", "02:15:2A:01:02:00"]
    error_message = "slot 42's router MACs must carry the slot (2A)"
  }
}

# The node fix (Phase 3): no default node (it was `pve`); an empty one is refused.
run "an_empty_node_is_refused" {
  command = plan

  variables {
    node = ""
  }

  expect_failures = [var.node]
}

# The spec: `back` has `internet: false`, so its machines may not reach the internet. The router
# forwards and masquerades every network to the internet (the module says why: machines install
# their software, and an offline machine blocks the internet itself once set up), so only cache's
# own nftables rule (its `closing the internet` step) stops it, which root on cache can lift.
run "known_bug_the_router_forwards_a_network_without_internet_to_it" {
  command = plan

  assert {
    condition     = length([for l in split("\n", one([for f in yamldecode(trimprefix(proxmox_virtual_environment_file.router.source_raw[0].data, "#cloud-config\n")).write_files : f.content if f.path == "/etc/nftables.conf"])) : l if strcontains(l, "daddr != {") && strcontains(l, "10.61.20.0/24")]) == 0
    error_message = "the router must not forward nor masquerade back (internet: false) to the internet"
  }
}
