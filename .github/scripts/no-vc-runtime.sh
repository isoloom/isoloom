#!/usr/bin/env bash
# Refuses a Windows executable that needs Microsoft's Visual C++ runtime: a clean Windows without
# the Visual C++ Redistributable can't start it (exit 0xC0000135, a DLL not found). The release
# links the C runtime in (`+crt-static`, release.yml); this holds it there. GitHub's Windows
# runners have the redistributable, so the release's install checks can't tell.
#
# Usage: no-vc-runtime.sh <llvm-objdump> <exe>
# Exit 0: it imports none of the runtime's DLLs (VCRUNTIME*.dll, MSVCP*.dll). Exit 1: it does.
set -euo pipefail

objdump="${1:?llvm-objdump}"
exe="${2:?the executable}"

dlls="$("$objdump" -p "$exe" | sed -n 's/^[[:space:]]*DLL Name:[[:space:]]*//p')"
echo "${exe} imports: $(echo "$dlls" | tr '\n' ' ')"
runtime="$(echo "$dlls" | grep -iE '^(vcruntime|msvcp)[0-9a-z_]*\.dll$' || true)"
if [ -n "$runtime" ]; then
  echo "::error title=VC++ runtime::${exe} imports $(echo "$runtime" | tr '\n' ' ')- a Windows without the Visual C++ Redistributable can't start it; link the C runtime in (-C target-feature=+crt-static)"
  exit 1
fi
