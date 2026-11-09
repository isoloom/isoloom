# Kubernetes

The `kubernetes` target writes the environment three ways, under `.isoloom/kubernetes/`:

| File | Use it with |
|---|---|
| `environment.yaml` | `kubectl apply -f`: a namespace, a Deployment and Services per machine, NetworkPolicies for `reach` and `internet: false` |
| `kustomization.yaml` | `kubectl apply -k`, or as a base of your own overlay |
| `chart/` | `helm install`, `helm test` (the checks), `helm package` |
| `checks/` | `kubectl apply -k .isoloom/kubernetes/checks`: the checks as Jobs |

Tier 2: the files pass `kubeconform` and `helm lint`, and CI runs three examples on a kind
cluster with Calico, but no real run from every host OS is recorded. There is no run manifest:
you run the files with `kubectl` or Helm.

## Prerequisites

- A cluster and `kubectl` (with its built-in Kustomize), or Helm 3 for the chart.
- A network plugin that enforces NetworkPolicies, egress included (Calico, Cilium). Without one
  the policies are accepted and ignored: `reach` and `internet: false` are not enforced, and the
  derived checks fail.
- A registry the cluster can pull from, for images built from the project.
- Rights to create a namespace and, in it, Deployments, Services, NetworkPolicies, ConfigMaps,
  PersistentVolumeClaims, Jobs and Pods (and Secrets, when the spec has secrets).

## Generate and run

```sh
isoloom generate --target kubernetes
```

Build the project's images and push them where the cluster pulls from. The header of
`environment.yaml` lists the `docker build` commands; images built from the project are named
under `localhost/isoloom/` by default. A local cluster that shares Docker's images (Docker
Desktop, OrbStack) needs no push; for kind, `kind load docker-image <image>`.

```sh
kubectl apply -k .isoloom/kubernetes
kubectl -n isoloom-<name> wait --for=condition=Available deployment --all --timeout=600s
kubectl apply -k .isoloom/kubernetes/checks
kubectl -n isoloom-<name> wait --for=condition=complete job --all --timeout=300s
kubectl -n isoloom-<name> logs -l isoloom.com/check --tail=-1
```

With Helm:

```sh
helm install env .isoloom/kubernetes/chart -n isoloom-<name> --create-namespace
helm test env -n isoloom-<name> --logs
```

## Published ports

A port with `publish:` is a ClusterIP Service by default: reach it with
`kubectl -n isoloom-<name> port-forward svc/<machine>-published <local>:<port>`. Exposing it
outside the cluster (a LoadBalancer Service, an Ingress) is a decision for your cluster's
operators: add it in a Kustomize overlay or a manifest of your own, with the source ranges your
policy allows.

## Pod security

The pods run under the Pod Security Standards' `baseline` level, which the namespace enforces: the
runtime's seccomp filter, no privilege escalation, every capability dropped but those a root
process of a stock image needs (`NET_RAW` among the dropped), Isoloom's own helpers as `nobody`. A
machine that is `privileged`, or sets a sysctl outside the safe set, makes the namespace
`privileged` instead. A cluster that admits only `restricted` pods refuses them (a stock image
runs as root). Isoloom labels every object `app.kubernetes.io/managed-by: isoloom`.

## Teardown

```sh
kubectl delete namespace isoloom-<name>
helm uninstall env -n isoloom-<name>     # for the chart
```

## Cost

Whatever your cluster costs. The pods set no CPU or memory requests; `isoloom resources` adds up
what the machines ask for (1 CPU, 512 MB and 5 GB each as containers, unless the spec says).
