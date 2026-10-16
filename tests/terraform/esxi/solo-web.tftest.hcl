# examples/solo-web on ESXi: one machine on one network (a port group on a vSwitch of its own with
# no uplink), its web port (80, http) published as 8080 at the router's uplink address: the router
# forwards 8080 from outside the lab to web:80 and the internet out. Every run plans against
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

run "the_network_is_a_port_group_the_router_stands_on_at_254" {
  command = plan

  assert {
    condition     = esxi_vswitch.app.name == "iso1-app" && length(esxi_vswitch.app.uplink) == 0 && esxi_portgroup.app.vswitch == "iso1-app"
    error_message = "app must be a port group on a vSwitch of its own, without uplink"
  }
  assert {
    condition     = [for n in esxi_guest.isoloom_router.network_interfaces : n.virtual_network] == ["VM Network", "iso1-app"] && yamldecode(base64decode(esxi_guest.isoloom_router.guestinfo["metadata"])).network.ethernets.nic1.addresses == ["10.80.0.254/24"]
    error_message = "the router must be on uplink_port_group first, then on app at 10.80.0.254"
  }
  assert {
    condition     = [for n in esxi_guest.web.network_interfaces : n.virtual_network] == ["iso1-app"] && yamldecode(base64decode(esxi_guest.web.guestinfo["metadata"])).network.ethernets.nic0.addresses == ["10.80.0.10/24"] && yamldecode(base64decode(esxi_guest.web.guestinfo["metadata"])).network.ethernets.nic0.routes == [{ to = "0.0.0.0/0", via = "10.80.0.254" }]
    error_message = "web must be on app at 10.80.0.10, through the router"
  }
}

run "the_router_forwards_the_published_port_to_web_and_the_network_out" {
  command = plan

  assert {
    condition = split("\n", one([for f in yamldecode(trimprefix(base64decode(esxi_guest.isoloom_router.guestinfo["userdata"]), "#cloud-config\n")).write_files : f.content if f.path == "/etc/nftables.conf"])) == tolist([
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

run "the_published_port_is_an_output_keyed_by_machine_and_port_name" {
  command = plan

  assert {
    condition     = output.published == { "web/web" = "192.0.2.60:8080" }
    error_message = "`published` must hold web's port `web` at the router's uplink address and the published port"
  }
  assert {
    condition     = output.ip == "192.0.2.60" && output.ready_file == "/var/lib/isoloom/ready" && output.machines == { web = "10.80.0.10" }
    error_message = "`ip` must be the router's uplink address, `ready_file` the isoloom-ready/1 file, `machines` web's lab address"
  }
}

# Without its uplink address the published port would point into the lab: the apply fails
# instead (Phase 3).
run "a_router_reporting_its_lab_address_fails_the_apply" {
  command = plan

  override_resource {
    target          = esxi_guest.isoloom_router
    override_during = plan
    values          = { ip_address = "10.80.0.254" }
  }

  expect_failures = [esxi_guest.isoloom_router]
}
