# invoice-portal-api

A small lab in the shape of a CTF lab a launcher starts: a read-only invoices API (`web`) and its
database (`database`), two Debian 12 machines on one network. The API's documentation names a
demo key; a request without the `X-Portal-Key` header gets 401, a request with it gets the invoice.

What it exercises, and what Isoloom's tests keep working for labs of this shape:

- **Two values given at launch, delivered two ways.** `CTF_API_URL` isn't secret: it reaches the
  database as an environment value. `CTF_LAUNCH_TOKEN` is: it reaches the database as the file
  `/run/lab/launch-token`, never as an environment variable.
- **A fixed, non-privileged host port.** The API listens on 80 inside its machine and is
  published on the host's 8080 (`fixed: true`), the URL its documentation gives.
- **A recipe per form for both machines.** A `build` on containers (an image from `mysql:8.0`,
  and one from `python:3.12-slim`) and `steps` on VMs (MariaDB, and the same app as a systemd
  service), so the lab runs on containers, Vagrant, Proxmox, ESXi and the clouds.
- **A setup step and a volume.** Once the database answers, `build/database/evidence.sh` places
  the amount of invoice INV-1042 and keeps it in the `evidence` volume across restarts.
- **Both kinds of check.** An `http` check (no key: 401) and a script (the documented path).
- **A message** with the published URL.

How it differs from `supplier-portal-api`, which also delivers a launch token as a file:
that example is the smallest form of the idea (an image for the database, an nginx page on a
port published as itself, one script check). This one mirrors a published lab's layout: both
machines built from their own recipes, the API on 80 published on 8080, a volume the setup step
writes to, an `http` check next to the script, and a message.

Every value here is a placeholder: the demo key, the database passwords and the invoices are
made up, and without a launch token the database seeds its development amount (1234.00). With
one, the setup step derives the amount from the token locally; a real lab would claim it from
`CTF_API_URL` instead.

```sh
isoloom targets                    # where it runs
isoloom generate --target docker   # the Compose files, in .isoloom/docker
# once it is up:
curl -H 'X-Portal-Key: example-demo-key' http://localhost:8080/api/invoices/INV-1042
```
