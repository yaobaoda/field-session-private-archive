#!/bin/sh
set -eu

: "${INFRAI_API_KEY:?set INFRAI_API_KEY}"
export INFRAI_BUCKET="${INFRAI_BUCKET:-field-session-archive}"

cargo run --bin field-session -- "${1:-wo-1842}" "${2:-tech-17}"
