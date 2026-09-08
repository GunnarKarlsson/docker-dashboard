#!/bin/sh
set -eu

mkdir -p /data
echo "INFO db ready data_dir=/data"

while true; do
  date -u +"INFO db checkpoint ok ts=%Y-%m-%dT%H:%M:%SZ" >> /data/checkpoint.log
  echo "INFO db checkpoint ok"
  sleep 10
done
