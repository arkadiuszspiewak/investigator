# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim

WORKDIR /opt/investigator
COPY agents/bedrock/package.json agents/bedrock/package-lock.json ./
RUN npm ci --omit=dev \
    && apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 65532 --shell /usr/sbin/nologin agent \
    && install --directory --mode=0555 /workspace

COPY agents/bedrock/entrypoint.sh /usr/local/bin/investigator-agent
COPY agents/bedrock/runner.mjs /opt/investigator/runner.mjs
COPY agents/bedrock/AGENTS.md /workspace/AGENTS.md
RUN chmod 0755 /usr/local/bin/investigator-agent \
    && chmod 0555 /opt/investigator/runner.mjs \
    && chmod 0444 /workspace/AGENTS.md

USER 65532:65532
WORKDIR /workspace
ENV HOME=/home/agent

ENTRYPOINT ["/usr/local/bin/investigator-agent"]
