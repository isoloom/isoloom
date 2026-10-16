# examples/private-names on Proxmox: `dns.private` with `corp.internal`, web (alias www) and client
# on one network. Proxmox has no private DNS of its own, so the names are in every VM's hosts
# file (its `hosts file` step), and the domain is each VM's search domain. Every run plans against
# mocked providers; the router's uplink address and the snippets' ids are pinned below.

mock_provider "proxmox" {
  override_during = plan
}

mock_provider "tls" {
  override_during = plan
}

override_resource {
  target          = proxmox_virtual_environment_vm.isoloom_router
  override_during = plan
  values          = { ipv4_addresses = [["127.0.0.1"], ["192.0.2.50"], ["10.84.0.254"]] }
}

override_resource {
  target          = proxmox_virtual_environment_file.web
  override_during = plan
  values          = { id = "local:snippets/iso1-web.yaml" }
}

override_resource {
  target          = proxmox_virtual_environment_file.client
  override_during = plan
  values          = { id = "local:snippets/iso1-client.yaml" }
}

variables {
  proxmox_endpoint = "https://pve.test:8006/"
  node             = "pve-test"
  ssh_public_key   = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "each_vm_s_search_domain_is_the_zone_s" {
  command = plan

  assert {
    condition     = proxmox_virtual_environment_vm.web.initialization[0].dns[0].domain == "corp.internal" && proxmox_virtual_environment_vm.client.initialization[0].dns[0].domain == "corp.internal"
    error_message = "web's and client's search domain must be dns.domain, corp.internal"
  }
}

run "each_vm_s_hosts_file_names_the_others_in_the_zone" {
  command = plan

  assert {
    condition     = tolist(flatten(regexall("'\\\\''([0-9.]+ [^']*)'\\\\''", element(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 1)))) == tolist(["10.84.0.20 client client.corp.internal"])
    error_message = "web's hosts file must give client by its name and its name in the zone"
  }
  assert {
    condition     = tolist(flatten(regexall("'\\\\''([0-9.]+ [^']*)'\\\\''", element(yamldecode(trimprefix(proxmox_virtual_environment_file.client.source_raw[0].data, "#cloud-config\n")).runcmd, 1)))) == tolist(["10.84.0.10 web www web.corp.internal www.corp.internal"])
    error_message = "client's hosts file must give web by its name, its alias www, and both in the zone"
  }
  assert {
    condition     = startswith(element(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 1), "echo 'running: hosts file' > /var/lib/isoloom/ready; ")
    error_message = "the hosts file must be a set-up step that says so in the ready file"
  }
}

# The spec (`dns.private`): where the platform has no private DNS, the hosts file every VM gets
# carries the same names as the zone: every machine's name and aliases under the domain. Each
# VM's hosts file leaves its own names out, so on web `www.corp.internal` (its own alias in the
# zone) doesn't resolve, as it does on the clouds' zones.
run "known_bug_a_vm_s_hosts_file_leaves_its_own_names_in_the_zone_out" {
  command = plan

  assert {
    condition     = contains(flatten(regexall("'\\\\''([0-9.]+ [^']*)'\\\\''", element(yamldecode(trimprefix(proxmox_virtual_environment_file.web.source_raw[0].data, "#cloud-config\n")).runcmd, 1))), "10.84.0.10 web www web.corp.internal www.corp.internal")
    error_message = "web's hosts file must give its own names in the zone too (web.corp.internal, www.corp.internal)"
  }
}
