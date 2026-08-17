#!/bin/sh
set -eu

if [ "${INVESTIGATOR_BEDROCK_WORKLOAD_IDENTITY:-}" = "true" ]; then
  BEDROCK_API_KEY=$(node /opt/investigator/bedrock-token.mjs)
  export BEDROCK_API_KEY
fi

if [ -z "${OPENAI_API_KEY:-}" ] && [ -z "${BEDROCK_API_KEY:-}" ] && [ ! -f "${CODEX_HOME}/auth.json" ]; then
  echo "Codex credentials are missing: configure an OpenAI key, Bedrock key, workload identity, or ${CODEX_HOME}/auth.json" >&2
  exit 1
fi

if [ -n "${INVESTIGATOR_MCP_SERVERS:-}" ]; then
  printf '%s' "$INVESTIGATOR_MCP_SERVERS" | jq -e 'type == "array"' >/dev/null
  printf '%s' "$INVESTIGATOR_MCP_SERVERS" | jq -c '.[]' | while IFS= read -r server; do
    name=$(printf '%s' "$server" | jq -er '.name')
    url=$(printf '%s' "$server" | jq -er '.url')
    codex mcp add "$name" --url "$url"
  done
fi

exec codex "$@"
