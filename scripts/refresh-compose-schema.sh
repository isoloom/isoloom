#!/bin/sh
# Refreshes the Compose coverage source from the compose-spec repository:
# - crates/isoloom-coverage/coverage/compose-spec.json: the schema at the pinned commit;
# - crates/isoloom-coverage/coverage/compose-keys.txt: every key path it allows, one per line
#   (`*` for a name the file chooses, `[]` for a list item), the list the coverage table covers.
#
# Usage: sh scripts/refresh-compose-schema.sh   (then `cargo test`: new keys fail until classified)
# ISOLOOM_REFRESH=latest reads the schema on upstream's main branch instead and rewrites only the
# key list, so the drift job sees new features without the vendored schema moving off its pin.
# To move the pin: set COMMIT below, run without the variable, and update the commit named in
# crates/isoloom-coverage/src/compose/mod.rs.
set -eu
COMMIT=914ec15d1fa498969c0df5c1d672306db3256089
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/crates/isoloom-coverage/coverage"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

REF="$COMMIT"
if [ "${ISOLOOM_REFRESH:-}" = "latest" ]; then
  REF=main
fi
curl -fsSL "https://raw.githubusercontent.com/compose-spec/compose-spec/$REF/schema/compose-spec.json" -o "$WORK/schema.json"
if [ "$REF" = "$COMMIT" ]; then
  cp "$WORK/schema.json" "$OUT/compose-spec.json"
fi

# The same walk as crates/isoloom-coverage/tests/compose.rs: `$ref` and the combinators
# followed; `x-` extension keys left out (they are free-form, not features).
python3 - "$WORK/schema.json" "$OUT/compose-keys.txt" <<'EOF'
import json
import sys

schema = json.load(open(sys.argv[1]))
defs = schema["$defs"]


def alternatives(node, out):
    out.append(node)
    if "$ref" in node:
        alternatives(defs[node["$ref"].rsplit("/", 1)[1]], out)
    for combinator in ("allOf", "anyOf", "oneOf"):
        for sub in node.get(combinator, []):
            alternatives(sub, out)


def walk(node, prefix, out):
    alts = []
    alternatives(node, alts)
    for alt in alts:
        for key, sub in alt.get("properties", {}).items():
            path = f"{prefix}.{key}" if prefix else key
            out.add(path)
            walk(sub, path, out)
        for pattern, sub in alt.get("patternProperties", {}).items():
            if not pattern.startswith("^x-"):
                walk(sub, prefix + ".*", out)
        if isinstance(alt.get("additionalProperties"), dict):
            walk(alt["additionalProperties"], prefix + ".*", out)
        if isinstance(alt.get("items"), dict):
            walk(alt["items"], prefix + "[]", out)


paths = set()
walk(schema, "", paths)
with open(sys.argv[2], "w") as f:
    f.write("\n".join(sorted(paths)) + "\n")
print(f"compose: {len(paths)} key paths")
EOF
