# RFC: <short name>

- **Status:** Draft <!-- Draft, Accepted, Postponed, Declined, Implemented -->
- **Pull request:** <!-- filled in by the maintainer -->
- **Issue:** #
- **Tracking issue:** <!-- filled in on acceptance -->

## Summary

One paragraph: what changes, for whom.

## Motivation

The problem, with a concrete example: an environment someone wants to describe and can't,
or can only describe badly today. Why it belongs in Isoloom rather than in a project's own
scripts, image or provisioning.

## Design

What a user writes and what they get. Show the spec (a complete, small `isoloom.yml`) and the
commands, then what each target does with it:

| Target | Renders it as | Or refuses it, because |
|---|---|---|
| `docker` | | |
| `kubernetes` | | |
| `vagrant` | | |
| `proxmox` | | |
| `cloud-vm` | | |

Name the derived checks that prove it works, and the errors `isoloom validate` gives for the
mistakes people will make.

## Compatibility

Does a spec that validates today still validate, and behave the same? If a field is renamed or
removed, how does the deprecation go (see the [stability promise](../docs/stability.md))? Does
the supported library surface, the resolved snapshot, the registry or the JSON events change?

## Alternatives

Other designs considered, and why this one. What happens if we do nothing.

## Prior art

How Compose, Kubernetes, Vagrant, Terraform or other tools express the same thing.

## Open questions

What should be settled during the discussion, and what can wait for the implementation.
