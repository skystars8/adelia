#!/usr/bin/env sh
set -eu

mkdir -p /opt/adelia/generated /opt/adelia/data/uploads
chown -R adelia:adelia /opt/adelia/generated /opt/adelia/data/uploads

exec gosu adelia "$@"
