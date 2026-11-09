# Getting help

- **How do I…?** Start with the [documentation](../docs/README.md): the getting-started guide,
  the how-to guides, the spec reference and the target matrix. Questions are welcome in
  [Discussions](https://github.com/isoloom/isoloom/discussions).
- **Something doesn't work.** Search the [issues](https://github.com/isoloom/isoloom/issues),
  then open one with the *Bug report* form. Include `isoloom --version`, your OS, the target, the
  smallest `isoloom.yml` that shows the problem, the command and its output. Remove secrets from
  what you paste.
- **An idea or a missing feature.** Open an issue with the *Feature request* form, or a
  *Catalogue entry* request for a packaged service.
- **A security vulnerability.** Don't open an issue: follow [SECURITY.md](SECURITY.md).
- **A problem while running an environment** (bringing it up, checks, teardown, the reaper): the
  runner's own issues, for Isothrum at [Isothrum/isothrum](https://github.com/Isothrum/isothrum).
  A wrong generated file (Compose file, Vagrantfile, Terraform module, run manifest) is Isoloom's.

Isoloom is maintained on a best-effort basis: there is no paid support and no guaranteed response
time, except for security reports ([SECURITY.md](SECURITY.md)). Which releases get fixes:
[supported releases](../docs/reference/policies.md#supported-releases).
