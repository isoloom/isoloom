# A public S3 website: the smallest cloud-services environment.
terraform {
  required_providers {
    aws    = { source = "hashicorp/aws", version = "~> 6.0" }
    random = { source = "hashicorp/random", version = "~> 3.6" }
  }
}

variable "region" {
  type    = string
  default = "us-east-1"
}

provider "aws" {
  region = var.region
  default_tags {
    tags = { "isoloom-environment" = "cloud-bucket", "managed-by" = "isoloom" }
  }
}

resource "random_id" "suffix" {
  byte_length = 4
}

resource "aws_s3_bucket" "site" {
  bucket        = "isoloom-cloud-bucket-${random_id.suffix.hex}"
  force_destroy = true
}

resource "aws_s3_bucket_public_access_block" "site" {
  bucket                  = aws_s3_bucket.site.id
  block_public_acls       = true
  ignore_public_acls      = true
  block_public_policy     = false
  restrict_public_buckets = false
}

resource "aws_s3_bucket_policy" "public_read" {
  bucket     = aws_s3_bucket.site.id
  depends_on = [aws_s3_bucket_public_access_block.site]
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = "*", Action = "s3:GetObject", Resource = "${aws_s3_bucket.site.arn}/*" }]
  })
}

resource "aws_s3_bucket_website_configuration" "site" {
  bucket = aws_s3_bucket.site.id
  index_document {
    suffix = "index.html"
  }
}

resource "aws_s3_object" "index" {
  bucket       = aws_s3_bucket.site.id
  key          = "index.html"
  content      = "<!doctype html><p>hello from isoloom</p>\n"
  content_type = "text/html"
}

output "website_url" {
  value = "http://${aws_s3_bucket_website_configuration.site.website_endpoint}/"
}
