# Isoloom and Testcontainers

[Testcontainers](https://testcontainers.com/) is a library, for Java, Go, .NET, Python, Node.js
and other languages, that starts throwaway containers from your test code: a database for one
test class, a message broker for one suite.

## What Testcontainers does better

- **It lives in your tests.** Containers start and stop with a test, configured in the test's
  own language, with no file or command outside it.
- **Per-test isolation and speed.** Each test can get a fresh dependency on a random port the
  library hands back, and parallel tests don't collide.
- **Ready-made modules** for common services (databases, brokers, cloud emulators), with wait
  strategies that know when each is ready.
- **It fits unit and integration tests** run by your build tool, on any machine with a Docker
  API.

## What Isoloom adds

- **A whole environment, not one dependency.** Several machines on several networks, with
  `reach` enforced between them and link impairment (`tc`), described once in `isoloom.yml`.
- **More than containers.** The same spec runs as VMs (Windows machines included), on
  Kubernetes, Proxmox, ESXi or a cloud ([Targets](../targets.md)), for tests that need a real OS
  or network.
- **Checks from inside.** `isoloom test` probes from the machines' own positions: what a client
  on one network can reach, and what it can't.
- **The same environment by hand.** People can `isoloom run docker` the environment your tests
  use and look around, with `connect`, `exec` and `capture`.

## When to use which

- Use **Testcontainers** for the dependencies of an application's tests: a database or a broker,
  started per test or per suite from the test code.
- Use **Isoloom** for system and network tests of a multi-machine environment, tests that must
  also run on VMs or in a cloud, or a lab that people use as well as test.
- **Both:** start the environment before the test suite, run the suite against it, and tear it
  down after:

  ```sh
  isoloom run docker --json > run.jsonl   # the last line's data.message says where to connect
  ./gradlew integrationTest
  isoloom down docker
  ```

  `--instance N` gives each parallel job its own copy, with its own names and ports. Isoloom has
  no library to start an environment from test code.

## Bringing a project in

There is no importer for Testcontainers code: the containers are defined in a program. If the
project also has a Compose file (Testcontainers can start one), `isoloom import compose` drafts
the spec from it (see [Docker Compose](docker-compose.md#bringing-a-compose-project-in)).
Otherwise write the spec from the containers the tests start, starting from
[Getting started](../getting-started.md).
