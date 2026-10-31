#!/bin/sh
# Serves the built dashboard and API with a freshly generated fixture repository.
set -eu
root=$(cd ../.. && pwd)
work=$(mktemp -d)
mkdir "$work/fixture"
"$root/target/debug/generate-fixture" "$work/fixture"
printf '%s' "$work/fixture" > "$root/apps/web/e2e/.fixture-path"
exec "$root/target/debug/codeatlas" serve --port "$1" --data-dir "$work/data" --web-dir "$root/apps/web/dist"
