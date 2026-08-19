#!/bin/sh
set -eu

if [ -n "${INVESTIGATOR_MCP_SERVERS:-}" ]; then
  printf '%s' "$INVESTIGATOR_MCP_SERVERS" | jq -e 'type == "array"' >/dev/null
  printf '%s' "$INVESTIGATOR_MCP_SERVERS" | jq -c '.[]' | while IFS= read -r server; do
    name=$(printf '%s' "$server" | jq -er '.name')
    url=$(printf '%s' "$server" | jq -er '.url')
    codex mcp add "$name" --url "$url"
  done
fi

if [ "${1:-}" = "exec" ]; then
  exec node /opt/investigator/runner.mjs "$@"
fi

exec codex "$@"
