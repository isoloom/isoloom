# Keep Terraform state remotely

**Today the state is local only.** Every generated Terraform module declares `backend "local" {}`.
A runner keeps each run's state in its own state folder: the run manifest's `init` step passes
`-backend-config=path=${run.state_dir}/terraform.tfstate` and sets `TF_DATA_DIR` there, so
nothing lands in `.isoloom/`. Applied by hand, the state is `terraform.tfstate` in the module's
folder.

Local state is fine for short-lived environments a runner creates and destroys. For one a team
shares, or one that must survive the loss of a laptop or a CI runner, back up the runner's state
folder, or override the backend by hand as below.

## Override the backend by hand

Terraform merges files named `*_override.tf` over a module's own, and a `backend` in an override
file replaces the module's. Isoloom only manages the files it lists in `.isoloom/.generated`, so a
file you add next to the module survives `isoloom generate`, and `isoloom check` ignores it.

```hcl
# .isoloom/cloud-vm/aws/backend_override.tf (yours, not generated)
terraform {
  backend "s3" {
    bucket       = "corp-terraform-state"
    key          = "isoloom/hello-stack/cloud-vm-aws.tfstate"
    region       = "eu-west-3"
    use_lockfile = true
  }
}
```

```sh
terraform -chdir=.isoloom/cloud-vm/aws init
terraform -chdir=.isoloom/cloud-vm/aws apply -var allowed_cidr=203.0.113.7/32 \
  -var ssh_public_key="$(cat ~/.ssh/id_ed25519.pub)"
```

Any backend works the same way (`azurerm`, `gcs`, `http`, HCP Terraform's `cloud` block). Keep the
override file committed beside the generated files (and `.isoloom/.gitignore` ignoring only the
tools' state).

## Limits

- **Not with a runner, as generated.** The run manifest's `init` passes a `path` meant for the
  local backend, which another backend refuses; the runner also keeps the state's location per run.
  Use the override when you run the module with Terraform yourself.
- **One state per module.** Each target and cloud has its own module and needs its own `key`.
- **Secrets in state.** As with any Terraform state, it can hold sensitive values (generated
  passwords, the controller's private key): encrypt the backend and restrict who reads it.
