#!/bin/bash
# Emit output/build-manifest.json — traces a disk.qcow2 back to the exact
# source commit and container image that produced it. Run after a successful
# build+BIB, before the artifact is used for anything.
set -euo pipefail
cd "$(dirname "$0")/.."
M=podman-machine-default

VERSION=$(cat VERSION)
GIT_COMMIT=$(git rev-parse HEAD)
GIT_DIRTY=$([ -n "$(git status --porcelain)" ] && echo true || echo false)
CONTAINER_SHA=$(podman machine ssh $M "podman inspect --format '{{.Digest}}{{.Id}}' localhost/arkaos:dev 2>/dev/null | head -c 71" 2>/dev/null || echo unknown)
KERNEL=$(podman machine ssh $M "podman run --rm localhost/arkaos:dev ls /usr/lib/modules" 2>/dev/null | tr -d '\r' | head -1 || echo unknown)
ARTIFACT=output/qcow2/disk.qcow2
ARTIFACT_SHA=$( [ -f "$ARTIFACT" ] && sha256sum "$ARTIFACT" | cut -d' ' -f1 || echo missing)
ARTIFACT_BYTES=$( [ -f "$ARTIFACT" ] && stat -c%s "$ARTIFACT" || echo 0)

cat > output/build-manifest.json <<EOF
{
  "version": "$VERSION",
  "git_commit": "$GIT_COMMIT",
  "git_dirty": $GIT_DIRTY,
  "container_image": "$CONTAINER_SHA",
  "kernel": "$KERNEL",
  "artifact": "$ARTIFACT",
  "artifact_sha256": "$ARTIFACT_SHA",
  "artifact_bytes": $ARTIFACT_BYTES,
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
cat output/build-manifest.json
