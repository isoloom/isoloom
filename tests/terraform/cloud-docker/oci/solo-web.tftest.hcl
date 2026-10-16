# examples/solo-web on Oracle Cloud (cloud-docker): one host VM runs the docker target's Compose
# file, so the lab's networks live inside Compose; the cloud's part is its VCN, route table and security list: SSH and the published port 8080/tcp (web:80) from allowed_cidr. Every run plans against mocked providers;
# the ids the assertions compare are pinned below.

mock_provider "oci" {
  override_during = plan
  source          = "./tests/mocks/oci"
}

mock_provider "local" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = oci_core_vcn.env
  override_during = plan
  values          = { id = "ocid1.vcn.oc1..env" }
}

override_resource {
  target          = oci_core_internet_gateway.env
  override_during = plan
  values          = { id = "ocid1.internetgateway.oc1..env" }
}

override_resource {
  target          = oci_core_route_table.env
  override_during = plan
  values          = { id = "ocid1.routetable.oc1..env" }
}

override_resource {
  target          = oci_core_security_list.env
  override_during = plan
  values          = { id = "ocid1.securitylist.oc1..env" }
}

override_resource {
  target          = oci_core_subnet.env
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..env" }
}

override_resource {
  target          = oci_core_instance.env
  override_during = plan
  values          = { public_ip = "198.51.100.10" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  ssh_private_key_file = "/dev/null"
  compartment_id       = "ocid1.compartment.oc1..test"
}

run "the_host_sits_on_the_subnet_of_its_vcn" {
  command = plan

  assert {
    condition     = tolist(oci_core_vcn.env.cidr_blocks) == tolist(["10.42.0.0/16"]) && oci_core_subnet.env.cidr_block == "10.42.1.0/24" && oci_core_subnet.env.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "the host's subnet must be 10.42.1.0/24, in the environment's VCN (10.42.0.0/16)"
  }
  assert {
    condition     = oci_core_subnet.env.route_table_id == "ocid1.routetable.oc1..env" && tolist(oci_core_subnet.env.security_list_ids) == tolist(["ocid1.securitylist.oc1..env"])
    error_message = "the subnet must use the environment's route table and security list only"
  }
  assert {
    condition     = [for v in oci_core_instance.env.create_vnic_details : "${v.subnet_id} ${v.assign_public_ip}"] == ["ocid1.subnet.oc1..env true"]
    error_message = "the host must be on that subnet with a public address (its way out: Docker's install, and Terraform's SSH)"
  }
}

run "the_internet_gateway_is_the_host_s_way_out" {
  command = plan

  assert {
    condition     = oci_core_internet_gateway.env.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "the internet gateway must be in the environment's VCN"
  }
  assert {
    condition     = [for r in oci_core_route_table.env.route_rules : "${r.destination} ${r.network_entity_id}"] == ["0.0.0.0/0 ocid1.internetgateway.oc1..env"]
    error_message = "the route table must hold one route, the default one through the internet gateway"
  }
}

run "the_host_admits_ssh_and_the_published_port_from_allowed_cidr_only" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.env.ingress_security_rules : "${r.protocol} ${join(",", [for t in r.tcp_options : "${t.min}-${t.max}"])} ${r.source}"]) == sort([
      "6 22-22 203.0.113.7/32",
      "6 8080-8080 203.0.113.7/32",
    ])
    error_message = "the host must admit SSH and web's published port 8080/tcp from allowed_cidr, and nothing else"
  }
}

run "the_outputs_name_the_host_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.10"
    error_message = "`ip` must be the host's public address"
  }
  assert {
    condition     = output.ssh_user == "ubuntu"
    error_message = "`ssh_user` must be the host image's user"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file the set-up writes"
  }
}

# The spec: a published port is read as `machines.<m>.ports.<name>.published`, which the cloud
# targets give as the `published` output keyed `<machine>/<port-name>` (cloud-vm writes
# `{ "web/web" = "<ip>:8080" }`). The cloud-docker module writes no `published` output: only the
# firewall opens 8080.
run "known_bug_the_published_port_has_no_output" {
  command = plan

  assert {
    condition     = try(run.the_outputs_name_the_host_and_the_ready_file.published, null) == { "web/web" = "198.51.100.10:8080" }
    error_message = "the `published` output must give web's port `web` as { \"web/web\" = \"<ip>:8080\" }"
  }
}
