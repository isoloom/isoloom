# RFCs

An RFC (request for comments) is how a change that affects everyone who writes a spec, or
everyone who maintains a target, is proposed, discussed and decided before it is built. It is a
short Markdown file in this folder, discussed in its pull request.

## When an RFC is needed

Write one for:

- **Spec syntax changes:** a new top-level section, a new field on machines, networks or
  checks, a new meaning for an existing field, or a field renamed or removed. A new value in an
  existing list (another OS name, another cloud region) doesn't need one.
- **New targets:** another place to run an environment (a hypervisor, an orchestrator, a
  cloud), or a new way to run on an existing one that a user picks by name.
- **Catalogue format changes:** how a catalogue entry is written, versioned or proven, as
  opposed to adding or updating an entry.
- **Breaking changes:** anything that makes a spec that validates today fail, or behave
  differently, on a later release; a change to the supported library surface listed in the
  [README](../README.md#using-the-library); a change to a documented file format (the
  registry, the resolved snapshot, JSON events). [Stability](../docs/stability.md) says what
  counts as breaking.

No RFC is needed for a bug fix, a target producing a field it refused before, a new check,
documentation, a new catalogue entry, or internal refactoring. When unsure, open an issue or a
discussion first and ask.

## How it goes

1. **Talk first.** Open an issue or a discussion with the problem. It saves writing an RFC
   for something that already has an answer, or that the project doesn't want.
2. **Write it.** Copy [`0000-template.md`](0000-template.md) to `rfcs/0000-short-name.md` and
   fill it in. Keep the number at `0000`: the maintainer gives it the pull request's number
   on acceptance.
3. **Open a pull request** titled `RFC: <short name>`. The discussion happens there, on the
   lines of the file: comments, suggested changes, new revisions pushed to the branch. Mention
   the issue it comes from.
4. **Final comment period.** When the discussion has settled, a maintainer proposes to accept,
   postpone or decline, with a summary of the arguments, and leaves the pull request open for
   at least 7 more days so anyone can object.
5. **Decision.**
   - **Accepted:** the maintainer renames the file to the pull request's number, sets its
     status to Accepted and merges it. The design becomes the plan; a tracking issue is opened
     for the work.
   - **Postponed** or **declined:** the pull request is closed with the reasons. A postponed
     RFC can be reopened when what it waited for has changed.

An accepted RFC is a decision about what to build, not a promise of when. The implementation
comes in ordinary pull requests that link it. If building it shows the design was wrong, the
change goes back to the RFC in a new pull request before the code diverges.

## RFCs and decision records

An RFC is about what users and contributors see: the spec, the targets, the catalogue. The
[decision records](../docs/adr/) are about how Isoloom is built inside. An accepted RFC that
also changes the architecture gets an ADR when it is implemented.
