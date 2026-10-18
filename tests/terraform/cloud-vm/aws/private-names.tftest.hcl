# examples/private-names on AWS (cloud-vm): `dns.private` with `corp.internal`, web (alias www)
# and client on one network. The names are a Route 53 private zone attached to the VPC.

mock_provider "aws" {
  override_during = plan
  source          = "./tests/mocks/aws"
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = aws_vpc.env
  override_during = plan
  values          = { id = "vpc-env" }
}

override_resource {
  target          = aws_route53_zone.private
  override_during = plan
  values          = { zone_id = "Z-private" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "the_zone_is_private_to_the_vpc_which_resolves_through_it" {
  command = plan

  assert {
    condition     = aws_route53_zone.private.name == "corp.internal"
    error_message = "the zone must be dns.domain, corp.internal"
  }
  assert {
    condition     = [for v in aws_route53_zone.private.vpc : v.vpc_id] == ["vpc-env"]
    error_message = "the zone must be private to the environment's VPC"
  }
  assert {
    condition     = aws_vpc.env.enable_dns_support == true && aws_vpc.env.enable_dns_hostnames == true
    error_message = "the VPC must resolve through Route 53 (DNS support and hostnames on), or the private zone answers nobody"
  }
}

run "every_machine_and_alias_has_its_address_in_the_zone" {
  command = plan

  assert {
    condition = sort([
      "${aws_route53_record.web_corp_internal.name} ${aws_route53_record.web_corp_internal.type} ${join(",", aws_route53_record.web_corp_internal.records)} ${aws_route53_record.web_corp_internal.zone_id}",
      "${aws_route53_record.www_corp_internal.name} ${aws_route53_record.www_corp_internal.type} ${join(",", aws_route53_record.www_corp_internal.records)} ${aws_route53_record.www_corp_internal.zone_id}",
      "${aws_route53_record.client_corp_internal.name} ${aws_route53_record.client_corp_internal.type} ${join(",", aws_route53_record.client_corp_internal.records)} ${aws_route53_record.client_corp_internal.zone_id}",
      ]) == sort([
      "web.corp.internal A 10.84.0.10 Z-private",
      "www.corp.internal A 10.84.0.10 Z-private",
      "client.corp.internal A 10.84.0.20 Z-private",
    ])
    error_message = "web, its alias www and client must each have an A record at their address in the private zone"
  }
}

run "the_machines_admit_their_network_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in aws_security_group.web.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.84.0.0/24",
      "tcp 22-22 203.0.113.7/32",
    ])
    error_message = "web publishes nothing: it must admit office and SSH from allowed_cidr only"
  }
  assert {
    condition = sort([for r in aws_security_group.client.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.84.0.0/24",
      "tcp 22-22 203.0.113.7/32",
    ])
    error_message = "client must admit office and SSH from allowed_cidr only"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file"
  }
}
