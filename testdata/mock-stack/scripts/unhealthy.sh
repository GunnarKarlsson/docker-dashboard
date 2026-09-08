#!/bin/sh
set -eu

echo "WARN unhealthy sidecar starting"

while true; do
  echo "ERROR unhealthy healthcheck failing"
  sleep 5
done
