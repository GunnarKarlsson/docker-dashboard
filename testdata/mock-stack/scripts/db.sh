#!/bin/sh
set -eu

mkdir -p /data
if [ ! -f /data/blob ]; then
  dd if=/dev/urandom of=/data/blob bs=1M count=2 2>/dev/null
fi
echo "INFO db ready data_dir=/data"

while true; do
  date -u +"INFO db checkpoint ok ts=%Y-%m-%dT%H:%M:%SZ" >> /data/checkpoint.log
  echo "INFO db checkpoint ok"
  sleep 10
done
