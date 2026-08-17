# Architecture

```text
CLI ───────────────┐
                   ├──> Investigation CR ──> investigator controller ──> Codex Job per turn
Alertmanager ─> investigator-alerts ─┘                                  │
                   └──────────────────────────────────────────────> Slack
                                                    │
                    ┌───────────────────────────────┼──────────────┐
                    ▼                               ▼              ▼
              kubernetes-mcp                 prometheus-mcp    argo-mcp
```

- `investigator` is the core Kubernetes controller, not an MCP server and not the LLM.
  It converts desired state into Jobs and reports Job state on the CR.
- The agent image contains Codex plus an entrypoint that consumes
  `INVESTIGATOR_MCP_SERVERS`, configures Codex, and executes the query.
- Helm derives the controller-owned MCP registry from enabled server entries.
  Every Investigation uses that registry; CRs and clients cannot override MCP
  endpoints.
- Each MCP server owns one integration and is independently versioned, secured,
  scaled, and deployed.
- `prometheus-mcp` stays read-only and exposes only PromQL, metric/label
  discovery, and scrape-target health through the Prometheus HTTP API.
- `alertmanager-mcp` exposes only active alerts, grouping, silences, receivers,
  and status through read-only Alertmanager API calls.
- `mcp-runtime` contains transport mechanics only. Provider clients and tool
  schemas stay with their server.
- `investigator-cli` is a local conversational client. It appends questions to
  the CR rather than running Codex locally.
- `investigator-alerts` is an Alertmanager webhook adapter and notification
  boundary. Alert fingerprints combined with `startsAt` make Investigation
  creation idempotent within a continuous firing episode while allowing a new
  Investigation when the same alert fires again after resolving.

## Conversation workflow

The initial prompt remains `spec.query`. Clients append stable `{id, query}`
items to `spec.questions`; they never rewrite earlier turns. The controller
answers the first unanswered item, includes prior question/answer pairs as
context, and appends `{questionId, result}` to `status.answers`. Separate owned
Jobs (`-agent`, then `-agent-qN`) preserve auditability and retry isolation.

## Adding an MCP server

Create `servers/<provider>-mcp` with its own manifest, binary, handler, and
tools. Use `mcp_runtime::serve_http` for shared HTTP behavior. Add the crate to
the workspace and deploy it independently. Enabling its Helm entry adds it to
the central controller registry used by all Investigations. No controller code
change is required.

## Lifecycle and security

The controller creates one owned Job per conversation turn, observes completion or failure,
and publishes status. A production iteration should add explicit re-run and
cancellation semantics, finalizers, conditions/timestamps, result persistence,
timeouts, resource limits, and admission validation.

The controller, MCP servers, and agent Jobs must use separate identities. The
controller manages Investigations and Jobs. Each MCP server gets only its
integration credentials. The globally configured `agent.serviceAccount` limits
the Job's direct cluster and AWS actions. Queries and MCP URLs are ordinary API data and must not contain
secrets; mount credentials from Secrets and restrict egress with NetworkPolicy.
Node selectors, affinity, and tolerations for agent Jobs are global controller
configuration rendered from `agent.job` Helm values. Provider, model,
authentication, and workload identity are also global `agent` settings. They are not
part of the Investigation API, preventing individual clients from overriding
cluster placement policy.

## Packaging and releases

`charts/investigator-platform` is the only chart. Templates iterate over the
`mcpServers` map, while every server retains an independent image and release
cadence. `docker/Dockerfile.server` is the shared Rust server recipe;
`docker/Dockerfile.app` builds any Rust application by package and binary name;
MCP servers retain their separate `--http` runtime contract.

Each server directory has a small path-filtered workflow caller. All callers
invoke `_build-server.yml`, keeping registry login, tagging, caching, and image
construction consistent as the server list grows.
