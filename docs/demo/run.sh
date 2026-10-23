#!/usr/bin/env sh
set -eu
# Fictional values only. auth status reads these without making an API request.
cargo build -p ncal --quiet
printf '%s\n' 'Synthetic environment values; no sign-in or calendar API request.'
NCAL_USER_ID=demo-user \
NCAL_ACCESS_TOKEN=not-a-real-token \
NCAL_REFRESH_TOKEN= \
NCAL_TOKEN_EXPIRES_AT= \
./target/debug/ncal --config docs/demo/config.toml auth status
printf '\n%s\n' 'JSON output for the same local status:'
NCAL_USER_ID=demo-user \
NCAL_ACCESS_TOKEN=not-a-real-token \
NCAL_REFRESH_TOKEN= \
NCAL_TOKEN_EXPIRES_AT= \
./target/debug/ncal --config docs/demo/config.toml --json auth status
