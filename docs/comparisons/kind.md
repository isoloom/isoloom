# Isoloom and kind

[kind](https://kind.sigs.k8s.io/) (Kubernetes in Docker) runs a Kubernetes cluster whose nodes
are Docker containers. It makes a cluster; it doesn't describe an environment, so Isoloom and
kind fit together: Isoloom's `kubernetes` target applies its manifests to a cluster you already
have, and Isoloom's own CI runs that target on kind, with Calico for NetworkPolicies.

## What kind does better

- **It creates the cluster.** Isoloom doesn't: the `kubernetes` target needs `kubectl` and a
  current context that answers ([Targets](../targets.md#kubernetes)).
- **Testing Kubernetes itself.** Multi-node clusters, a chosen Kubernetes version, custom node
  images, feature gates, a CNI of your choice: kind is made for testing controllers, operators,
  CRDs and Kubernetes upgrades.
- **Anything Kubernetes can run.** Your Helm charts, operators and manifests as they are.
  Isoloom generates a Deployment and Service per machine plus NetworkPolicies, claims and check
  Jobs; it doesn't write ingresses, CRDs or operators.

## What Isoloom adds

- **Manifests from a portable spec.** The same spec runs on Docker Compose, VMs and clouds;
  `isoloom run kubernetes` turns it into a Namespace, a Deployment and Service per machine, and
  NetworkPolicies for `reach`. A Helm chart (with `helm test` checks) and a Kustomize overlay are
  written beside them.
- **Isolation that is proven.** The derived checks run as Jobs inside the cluster and prove each
  port answers from where `reach` allows and from nowhere else. That needs a CNI that enforces
  NetworkPolicies: kind's default one doesn't, which is why Isoloom's CI installs Calico.
- **The same environment elsewhere,** when a machine also needs to be a VM, or the lab needs to
  run where there is no cluster.

## When to use which

- Use **kind** alone to test Kubernetes software: controllers, charts, operators.
- Use **Isoloom on kind** to run an environment on Kubernetes locally or in CI, for example
  before running the same spec on a real cluster.
- Use **Isoloom without kind** when the environment doesn't need Kubernetes: the `docker` target
  is lighter for containers on one machine.

```sh
kind create cluster --config kind-calico.yaml   # with networking.disableDefaultCNI: true
kubectl apply -f https://raw.githubusercontent.com/projectcalico/calico/v3.29.1/manifests/calico.yaml
isoloom run kubernetes && isoloom test kubernetes && isoloom down kubernetes
```

Images built from the project must reach the cluster: on kind, `kind load docker-image` each one
after building it.

## Bringing a project in

kind's configuration describes a cluster, not an environment, so there is nothing to import from
it, and there is no importer for Kubernetes manifests or Helm charts. If the project has a
Compose file, `isoloom import compose` drafts a spec from it (see
[Docker Compose](docker-compose.md#bringing-a-compose-project-in)); otherwise write the spec from
the workloads you run, starting from [Getting started](../getting-started.md).
