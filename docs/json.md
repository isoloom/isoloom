# JSON output

Tools that drive Isoloom (a launcher, a control plane, a desktop app) read its JSON output
rather than its human-readable lines. Every lifecycle command takes `--json`:

| Command | With `--json` |
|---|---|
| `run`, `down`, `reset`, `provision`, `test`, `capture` | Events, one JSON object per line (newline-delimited JSON), the last a `result` event |
| `status` | One document: every registered environment with its live state (`--cleanup`: events) |
| `exec`, `tc`, `gc` | One document with what each machine or place answered, or what was collected |
| `connect` | One document: the command that opens the shell (program, arguments, folder), not run |

When a command fails, its last line is the error document, `{ "error": { ... } }`, and the exit
code is 2 as without `--json`. Exit code 1 means the command ran and the outcome is negative (a
check failed, a machine's command failed); 0 is success.

With `--json`, stdout carries only JSON. The tools' own output (Compose's progress, Vagrant's,
Terraform's) comes as `log` events; notes and warnings Isoloom adds stay on stderr.

The JSON Schema of an event line and of the error document is
[`docs/events.schema.json`](events.schema.json), also printed by `isoloom schema --events` and
published at `https://www.isoloom.com/schema/events-v1.json`.

## Events

Every event has an `event` kind and a `time` (`YYYY-MM-DDTHH:MM:SSZ`, UTC). Fields are always
present, `null` when they don't apply.

```json
{"event":"step","target":"docker","machine":null,"step":"up","state":"running","detail":"/work/hello-stack","time":"2026-10-14T10:00:00Z"}
{"event":"log","target":"docker","machine":null,"step":"up","stream":"stderr","line":" Container hello-stack-web-1  Started","time":"2026-10-14T10:00:04Z"}
{"event":"step","target":"docker","machine":null,"step":"up","state":"done","detail":null,"time":"2026-10-14T10:00:09Z"}
{"event":"result","command":"run","ok":true,"data":{"target":"docker","cloud":null,"instance":null,"dir":"/work/hello-stack","message":"Open http://127.0.0.1:32771"},"time":"2026-10-14T10:00:09Z"}
```

- `step`: a step started (`state: running`), finished (`done`) or failed (`failed`, followed by
  the error document). `machine` names the machine when the step is about one. `detail` is a
  short human-readable note (a folder, the runner, a reason).
- `log`: a line the underlying tool printed during `step`, from its `stdout` or `stderr`.
- `check`: one check's result in `test`: `position` (where it ran from), `name`, `ok`, `detail`.
- `result`: the command's outcome, its last event: `command`, `ok`, and `data` (below).

### Steps

| Step | Command | What it covers |
|---|---|---|
| `prepare` | run, down, reset, test, provision | Load and validate the spec, choose the target (its id is the `done` event's `detail`), write its files |
| `ready` | run, reset | This host has what the target needs (`isoloom doctor`), and every secret can be read from its store |
| `secrets` | run, reset (Kubernetes) | The `isoloom-secrets` Secret applied from the secrets' stores, before the manifests |
| `provision` | run (external), provision | A machine's steps (`machine` set) or the environment's playbooks |
| `up`, `down` | run, down, reset, status `--cleanup` | The target's tool bringing the environment up or tearing it down |
| `baseline`, `restore` | run, reset (Vagrant) | Saving or restoring the baseline snapshot |
| `controller` | test, provision (local VMs) | Booting the halted controller VM |
| `checks` | test | One check runner (`detail`: where it runs) |
| `capture` | capture | tcpdump on the machine, each packet line a `log` event. The `result` comes when tcpdump ends by itself (`-- -c 100`); stopped by a signal, the stream just ends |

New steps can appear; treat an unknown one as progress.

### Result data

| Command | `data` |
|---|---|
| `run`, `down`, `reset` | `target`, `cloud`, `instance`, `dir`, `message` (the spec's message, filled in, after a run) |
| `test` | `target`, `passed`, `failed`, `unfinished` (runners that didn't end), `results` (each `position`, `name`, `ok`, `detail`) |
| `provision` | `target`, `machines` (the ones asked for; empty for all) |
| `capture` | `target`, `machine`, `network`, `address` |
| `status --cleanup` | `cleaned`: each environment torn down (`name`, `target`, `dir`, `ok`) |

## One-shot documents

- `status`: `{ "registry": <path>, "environments": [{ name, dir, target, instance, cloud, profile, started, state }] }`.
- `exec`: `{ "target", "results": [{ machine, ok, exit_code, stdout, stderr }] }`; a machine that
  couldn't be reached has `skipped` (the reason) instead.
- `tc`: `{ "target", "network", "action", "ok", "places": [{ who, ok, output }] }`.
- `gc`: `{ "networks": [{ name, environment }], "volumes": [...], "images": [{ id, name }], "removed", "failed" }`.
- `connect`: `{ "target", "machine", "program", "args", "dir" }`.

## Errors

```json
{"error":{"code":"host_not_ready","message":"this machine can't run `docker` yet: docker isn't installed (see `isoloom doctor`)","machine":null,"step":"ready","hint":"isoloom doctor","problems":[]}}
```

`message` is what the command prints after `error:` without `--json`. `step` is the step that
was running, `machine` the machine it is about, `hint` what to do when there is more to say, and
`problems` the spec's problems (`at`, `message`) for `spec_invalid`.

| Code | Meaning |
|---|---|
| `usage` | The arguments don't make sense together (an unknown action, no command after `--`) |
| `spec_load` | No spec in the folder, or it can't be read or parsed |
| `spec_invalid` | The spec parses but has problems (listed in `problems`) |
| `profile` | The profile can't be read or doesn't fit the spec |
| `input_missing` | A launch-time input the environment needs isn't set (in the shell or the profile's `inputs`) |
| `secret_missing` | A secret the environment needs can't be read from its store (unset, unreadable, the store's command missing or failing); the message names each one |
| `target_unknown` | No target by that name |
| `target_unavailable` | The spec can't run on the target, or several are possible and none was named |
| `generate_failed` | The target's files couldn't be generated |
| `host_not_ready` | This host lacks a tool or a credential the target needs |
| `tool_missing` | A tool the target runs with isn't installed |
| `tool_failed` | The tool ran and failed (its output came as `log` events) |
| `not_running` | Nothing is running there: run the environment first |
| `machine_unknown` | No machine by that name |
| `network_unknown` | No network by that name, or the machine isn't on it |
| `unsupported` | The command doesn't do that on this target yet |
| `io` | Reading or writing a file failed |
| `internal` | Anything else |

Codes are stable: one never changes meaning. New codes can appear; treat an unknown one as
`internal` (the Rust types read it as `ErrorCode::Unknown`, and an unknown event kind as
`Event::Unknown`, rather than failing).

## Embedding

A Rust program embedding Isoloom gets the same events from `isoloom_runtime::events::Emitter`:

```rust
use isoloom_runtime::events::{Emitter, Event};

// A callback, called on the thread that emits (the tools' output is read on threads of its own)...
let emitter = Emitter::new(|e: &Event| println!("{e:?}"));
// ...or a channel.
let (emitter, events) = Emitter::channel();
```

`isoloom_runtime::tool::run_logged(&mut command, &emitter, &scope)` runs a tool with its output as
`log` events, as the command line does with `--json`; `Emitter::json_lines(writer)` writes them
as the command line prints them.
