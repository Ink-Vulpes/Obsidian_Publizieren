#!/usr/bin/env bash
# Baut das Docker-Image lokal und pusht es nach ghcr.io/ink-vulpes/obsidian-publisher
# Verwendung: bash push-image.sh [TAG]   (Standard-Tag: latest)

set -euo pipefail

IMAGE="ghcr.io/ink-vulpes/obsidian-publisher"
TAG="${1:-latest}"

echo "▶ Baue $IMAGE:$TAG …"
docker build -t "$IMAGE:$TAG" .

if [[ "$TAG" != "latest" ]]; then
  docker tag "$IMAGE:$TAG" "$IMAGE:latest"
fi

echo "▶ Pushe $IMAGE:$TAG …"
docker push "$IMAGE:$TAG"

if [[ "$TAG" != "latest" ]]; then
  echo "▶ Pushe $IMAGE:latest …"
  docker push "$IMAGE:latest"
fi

echo "✓ Fertig: $IMAGE:$TAG"
