#!/bin/sh
set -eu

i=0
echo "INFO api listening on 8080"

serve() {
  while true; do
    printf 'HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 3\r\nConnection: close\r\n\r\nok\n' | nc -l -p 8080
  done
}
serve &

while true; do
  echo "INFO api handled request id=${i}"
  if [ $((i % 5)) -eq 0 ]; then
    echo "WARN api slow response id=${i}"
  fi
  if [ $((i % 7)) -eq 0 ]; then
    echo "ERROR api failed to reach db id=${i}"
  fi
  i=$((i + 1))
  sleep 2
done
