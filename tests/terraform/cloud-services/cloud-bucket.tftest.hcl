# module: terraform
# examples/cloud-bucket on cloud-services: the user's own Terraform module (terraform/), which the
# generated scripts (.isoloom/cloud-services/up.sh, down.sh, outputs.sh) apply with the spec's
# `vars` (terraform.tfvars.json) and the deployment's instance (TF_VAR_isoloom_instance). These runs
# hold the contract between the spec and the module: the variables Isoloom sets exist, the output
# the spec's `outputs` names exists, and what the checks expect is what the module creates.

mock_provider "aws" {
  override_during = plan
}

mock_provider "random" {
  override_during = plan
}

override_resource {
  target          = random_id.suffix
  override_during = plan
  values          = { hex = "0a1b2c3d" }
}

override_resource {
  target          = aws_s3_bucket.site
  override_during = plan
  values          = { id = "isoloom-cloud-bucket-0a1b2c3d", arn = "arn:aws:s3:::isoloom-cloud-bucket-0a1b2c3d" }
}

override_resource {
  target          = aws_s3_bucket_website_configuration.site
  override_during = plan
  values          = { website_endpoint = "isoloom-cloud-bucket-0a1b2c3d.s3-website-us-east-1.amazonaws.com" }
}

# What the generated scripts pass: the spec's `vars` and the deployment's instance.
variables {
  region           = "us-east-1"
  isoloom_instance = "isoloom-cloud-bucket-t3st1d00"
}

run "the_spec_s_output_names_the_module_s_website" {
  command = plan

  assert {
    condition     = output.website_url == "http://isoloom-cloud-bucket-0a1b2c3d.s3-website-us-east-1.amazonaws.com/"
    error_message = "`website_url`, which the spec's `outputs.site` reads, must be the bucket's website address"
  }
}

run "the_bucket_serves_its_page_publicly_and_goes_with_the_environment" {
  command = plan

  assert {
    condition     = aws_s3_bucket.site.bucket == "isoloom-cloud-bucket-0a1b2c3d" && aws_s3_bucket.site.force_destroy == true
    error_message = "the bucket must be the environment's, and emptied on destroy so `isothrum down .` ends it"
  }
  assert {
    condition     = aws_s3_bucket_public_access_block.site.block_public_policy == false && aws_s3_bucket_public_access_block.site.restrict_public_buckets == false
    error_message = "the bucket must accept its public-read policy (the checks fetch the page from here)"
  }
  assert {
    condition     = jsondecode(aws_s3_bucket_policy.public_read.policy).Statement[0].Action == "s3:GetObject" && jsondecode(aws_s3_bucket_policy.public_read.policy).Statement[0].Resource == "arn:aws:s3:::isoloom-cloud-bucket-0a1b2c3d/*"
    error_message = "the policy must let anyone read the bucket's objects, and nothing more"
  }
  assert {
    condition     = [for d in aws_s3_bucket_website_configuration.site.index_document : d.suffix] == ["index.html"] && aws_s3_object.index.key == "index.html"
    error_message = "the website's index must be the page the module uploads"
  }
  assert {
    condition     = strcontains(aws_s3_object.index.content, "hello from isoloom")
    error_message = "the page must hold what the spec's check expects (`contains: hello from isoloom`)"
  }
}
