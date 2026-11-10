# Behind a corporate proxy, mirrors, registries, NTP and a corporate CA

Corporate networks often forbid direct internet access: outbound HTTP goes through a proxy,
images come from an internal registry, packages from internal mirrors, time from internal
servers, and TLS is inspected with a corporate certificate authority. The spec's `constraints:`
says so once, and every target applies it.

## The spec

```yaml
# isoloom.yml
version: 1
name: intranet
networks:
  lab: { cidr: 10.30.0.0/24 }
machines:
  web:
    os: debian@12
    networks: { lab: 10 }
    ports: { web: { port: 80, protocol: http } }
    app:
      image: "nginx:1.27"
      steps: [{ packages: [{ name: nginx }] }, { service: nginx }]
constraints:
  proxy:
    http: http://proxy.corp.example:3128
    no_proxy: [.corp.example, 10.0.0.0/8]
  registries:
    docker.io: registry.corp.example/dockerhub
  mirrors:
    debian: https://mirror.corp.example/debian
    pypi: https://pypi.corp.example/simple
  ntp: [ntp1.corp.example, ntp2.corp.example]
  dns: [10.0.0.53, 10.0.1.53]
```

| Key | Effect |
|---|---|
| `proxy.http`, `proxy.https`, `proxy.no_proxy` | Set for the containers and their builds, and on every Linux VM (the shell, apt, dnf, Docker). The environment's own machine names, networks and localhost are always added to `no_proxy`. Isoloom's own checks never use the proxy. |
| `registries` | Every image the environment pulls, Isoloom's own helper images included, is rewritten to the mirror: `docker.io: registry.corp.example/dockerhub` pulls `redis:7` as `registry.corp.example/dockerhub/library/redis:7`. Images built from the project aren't pulled. |
| `mirrors.debian`, `mirrors.ubuntu` | Replace the public archives in the VMs' apt sources |
| `mirrors.pypi`, `mirrors.npm` | `PIP_INDEX_URL` and `/etc/pip.conf`, `NPM_CONFIG_REGISTRY` and `/etc/npmrc` |
| `ntp` | The time servers Linux VMs synchronise with |
| `dns` | The resolvers (IP addresses) Isoloom's own machines and every VM naming none of its own ask, where the public `1.1.1.1` is blocked: the Proxmox and ESXi VMs, Isoloom's controller there, and the Proxmox router's resolver, which forwards the containers' queries to them |

The [target matrix](../matrix.md) shows which targets apply each key (every target but UTM and
`cloud-services` for the proxy and registries). Windows VMs get none of them yet. Field details:
[spec reference](../spec.md#constraints).

Keep these out of the spec when they differ per site: put them in a
[profile](../reference/configuration.md#profiles) (`profiles/<site>.yml`, under `spec:`), so one
spec runs at every site.

## A corporate certificate authority

Where TLS is inspected, machines must trust the corporate CA, or every `apt`, `pip` and image pull
through the proxy fails. `constraints.ca_certificates` lists CA certificates (PEM files in the
project) that the machines trust, added to their system trust store before they download anything.
The [spec reference](../spec.md#constraints) says which machines and targets take them.

```yaml
# in isoloom.yml
constraints:
  ca_certificates: [certs/corp-root-ca.pem]
```

Images pulled as they are (an official `redis` image) carry their own trust store: a mirror in
`registries` that serves them without inspection avoids the problem.

## The tools that run the generated files

The proxy and CA in the spec are for the machines. The tools on your own computer (Docker,
Vagrant, Terraform, a runner) use your environment's settings: `HTTPS_PROXY`, `NO_PROXY`, and your
system's trust store. Docker needs its own proxy configuration (Docker Desktop's settings, or the
daemon's), and Terraform a provider mirror when the registry is blocked
([offline](offline.md#running-without-internet-access)).
