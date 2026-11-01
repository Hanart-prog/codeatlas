#!/bin/sh
# Serves the built dashboard and API with a freshly generated fixture repository.
set -eu
root=$(cd ../.. && pwd)
work=$(mktemp -d)
mkdir "$work/fixture"
"$root/target/debug/generate-fixture" "$work/fixture"
printf '%s' "$work/fixture" > "$root/apps/web/e2e/.fixture-path"
mkdir "$work/scale" "$work/slow"
"$root/target/debug/generate-fixture" "$work/scale" --scale 3000 5
"$root/target/debug/generate-fixture" "$work/slow" --scale 200 1500
printf '%s' "$work/scale" > "$root/apps/web/e2e/.scale-fixture-path"
printf '%s' "$work/slow" > "$root/apps/web/e2e/.slow-fixture-path"
exec "$root/target/debug/codeatlas" serve --port "$1" --data-dir "$work/data" --web-dir "$root/apps/web/dist"
