# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim

ARG CODEX_VERSION=latest
RUN npm install --global "@openai/codex@${CODEX_VERSION}" \
    && apt-get update \
    && apt-get install --yes --no-install-recommends jq ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 65532 --shell /usr/sbin/nologin codex \
    && install --directory --owner=65532 --group=65532 /home/codex/.codex \
    && install --directory --mode=0555 /workspace

COPY --chmod=0755 agents/codex/entrypoint.sh /usr/local/bin/investigator-agent
COPY --chown=0:0 --chmod=0444 agents/codex/AGENTS.md /workspace/AGENTS.md

USER 65532:65532
WORKDIR /workspace
ENV HOME=/home/codex \
    CODEX_HOME=/home/codex/.codex

ENTRYPOINT ["/usr/local/bin/investigator-agent"]
