# examples/solo-web on Proxmox: one machine on one network (a VNet of the environment's SDN zone),
# its web port (80, http) published as 8080 at the router's uplink address: the router forwards
# 8080 from outside the lab to web:80 and the internet out. Every run plans against mocked
# providers; the VM id, the snippets' ids and the router's uplink address are pinned below.

mock_provider "proxmox" {
  override_during = plan
}

mock_provider "tls" {
  override_during = plan
}

override_resource {
  target          = proxmox_virtual_environment_vm.isoloom_router
  override_during = plan
  values          = { ipv4_addresses = [["127.0.0.1"], ["192.0.2.50"], ["10.80.0.254"]] }
}

override_resource {
  target          = proxmox_virtual_environment_vm.web
  override_during = plan
  values          = { vm_id = 101 }
}

override_resource {
  target          = proxmox_virtual_environment_file.router
  override_during = plan
  values          = { id = "local:snippets/iso1-router.yaml" }
}

override_resource {
  target          = proxmox_virtual_environment_file.web
  override_during = plan
  values          = { id = "local:snippets/iso1-web.yaml" }
}

variables {
  proxmox_endpoint = "https://pve.test:8006/"
  node             = "pve-test"
  ssh_public_key   = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "the_network_is_a_vnet_the_router_stands_on_at_254" {
  command = plan

  assert {
    condition     = proxmox_sdn_zone_simple.env.id == "iso1" && proxmox_sdn_vnet.app.id == "i1n0" && proxmox_sdn_vnet.app.zone == "iso1"
    error_message = "app must be a VNet of the slot's zone"
  }
  assert {
    condition     = tolist([for c in proxmox_virtual_environment_firewall_ipset.app.cidr : c.name]) == tolist(["10.80.0.0/24"])
    error_message = "app's IP set must hold its exact range"
  }
  assert {
    condition     = [for d in proxmox_virtual_environment_vm.isoloom_router.network_device : "${d.bridge} ${d.mac_address}"] == ["vmbr0 02:15:01:00:00:00", "i1n0 02:15:01:01:00:00"]
    error_message = "the router must be on uplink_bridge first, then on app's VNet"
  }
  assert {
    condition     = sort([for f in yamldecode(trimprefix(proxmox_virtual_environment_file.router.source_raw[0].data, "#cloud-config\n")).write_files : replace(f.content, "\n", " ") if startswith(f.path, "/etc/systemd/network/")]) == sort(["[Match] MACAddress=02:15:01:00:00:00  [Network] DHCP=yes", "[Match] MACAddress=02:15:01:01:00:00  [Network] Address=10.80.0.254/24 ConfigureWithoutCarrier=yes"])
    error_message = "the router must take its uplink by DHCP and 10.80.0.254 on app"
  }
}

run "the_router_forwards_the_published_port_to_web_and_the_network_out" {
  command = plan

  assert {
    condition = split("\n", one([for f in yamldecode(trimprefix(proxmox_virtual_environment_file.router.source_raw[0].data, "#cloud-config\n")).write_files : f.content if f.path == "/etc/nftables.conf"])) == tolist([
      "flush ruleset",
      "table inet isoloom {",
      "  chain forward {",
      "    type filter hook forward priority 0; policy drop;",
      "    ct state established,related accept",
      "    ip daddr 10.80.0.10 tcp dport 80 ct status dnat accept",
      "    ip saddr { 10.80.0.0/24 } ip daddr != { 10.80.0.0/24 } accept",
      "  }",
      "  chain prerouting {",
      "    type nat hook prerouting priority -100;",
      "    ip saddr != { 10.80.0.0/24 } tcp dport 8080 dnat ip to 10.80.0.10:80",
      "  }",
      "  chain postrouting {",
      "    type nat hook postrouting priority 100;",
      "    ip saddr { 10.80.0.0/24 } ip daddr != { 10.80.0.0/24 } masquerade",
      "  }",
      "}",
      "",
    ])
    error_message = "the router must forward 8080/tcp from outside the lab to web:80 (and only what it forwarded), and app (internet: true) out"
  }
}

run "web_is_on_app_behind_a_firewall_open_to_app_and_its_published_port" {
  command = plan

  assert {
    condition     = [for d in proxmox_virtual_environment_vm.web.network_device : "${d.bridge} ${d.firewall}"] == ["i1n0 true"] && proxmox_virtual_environment_vm.web.initialization[0].ip_config[0].ipv4[0].address == "10.80.0.10/24" && proxmox_virtual_environment_vm.web.initialization[0].ip_config[0].ipv4[0].gateway == "10.80.0.254"
    error_message = "web must be on app's VNet at 10.80.0.10, behind the firewall, through the router"
  }
  assert {
    condition     = proxmox_virtual_environment_firewall_options.web.vm_id == 101 && proxmox_virtual_environment_firewall_options.web.enabled && proxmox_virtual_environment_firewall_options.web.input_policy == "DROP"
    error_message = "web's firewall must be on, dropping what no rule accepts"
  }
  assert {
    condition = proxmox_virtual_environment_firewall_rules.web.vm_id == 101 && sort([for r in proxmox_virtual_environment_firewall_rules.web.rule : "${r.type} ${r.action} ${coalesce(r.source, "-")} ${coalesce(r.dest, "-")} ${coalesce(r.proto, "-")} ${coalesce(r.dport, "-")} ${coalesce(r.macro, "-")}"]) == sort([
      "in ACCEPT +iso1n0 - - - -",
      "in ACCEPT - - tcp 80 -",
    ])
    error_message = "web must admit app, and its published port's own port (80/tcp, which the router forwards 8080 to), and nothing else"
  }
}

run "the_published_port_is_an_output_keyed_by_machine_and_port_name" {
  command = plan

  assert {
    condition     = output.published == { "web/web" = "192.0.2.50:8080" }
    error_message = "`published` must hold web's port `web` at the router's uplink address and the published port"
  }
  assert {
    condition     = output.ip == "192.0.2.50" && output.ready_file == "/var/lib/isoloom/ready" && output.machines == { web = "10.80.0.10" }
    error_message = "`ip` must be the router's uplink address, `ready_file` the isoloom-ready/1 file, `machines` web's lab address"
  }
}

run "web_reports_its_set_up_in_the_ready_file" {
  command = plan

  assert {
    condition     = [for l in slice(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 1, 4) : regex("^echo 'running: ([^']*)' > /var/lib/isoloom/ready; ", l)[0]] == ["hosts file", "provision/web.sh", "health check"]
    error_message = "web's steps must each say `running: <step>` in the ready file first (its own name in the zone among them)"
  }
  assert {
    condition     = alltrue([for l in slice(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 1, 4) : regex("^echo 'running: ([^']*)'", l)[0] == regex("\\|\\| \\{ echo 'failed: ([^']*)' > /var/lib/isoloom/ready; exit 1; \\}$", l)[0]])
    error_message = "each of web's steps must write `failed: <step>` and stop when it fails"
  }
  assert {
    condition     = strcontains(element(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 3), "echo ready > /var/lib/isoloom/ready")
    error_message = "web's set-up must end by writing `ready`"
  }
}

# Without its uplink address, the published port has nowhere to be: the apply fails (Phase 3).
run "a_router_without_an_uplink_address_fails_the_apply" {
  command = plan

  override_resource {
    target          = proxmox_virtual_environment_vm.isoloom_router
    override_during = plan
    values          = { ipv4_addresses = [["127.0.0.1"], ["10.80.0.254"]] }
  }

  expect_failures = [proxmox_virtual_environment_vm.isoloom_router]
}
