# Investigator

A Rust workspace for an in-cluster Codex investigator and the MCP servers it
uses. MCP servers expose focused capabilities; the `investigator` controller
turns `Investigation` resources into short-lived Codex agent Jobs.

## Layout

```text
apps/investigator/          Investigation CRD and Job controller
apps/investigator-cli/      interactive desktop/terminal client
apps/investigator-alerts/   Alertmanager webhook to Investigation + Slack
crates/mcp-runtime/         shared MCP Streamable HTTP transport
servers/kubernetes-mcp/    read-only Kubernetes MCP server and tools
servers/prometheus-mcp/    read-only PromQL and target-health MCP server
servers/alertmanager-mcp/  read-only alerts and Alertmanager status MCP server
charts/investigator-platform/ central controller + MCP server chart
docker/                     reusable app/server and agent Dockerfiles
deploy/examples/            example Investigation resources
docs/architecture.md        boundaries and extension conventions
```

Future servers such as Prometheus or Argo belong in `servers/<name>-mcp` as
independent binaries. They may share infrastructure crates, but should not be
linked into one large MCP process.

## Build and run

```sh
cargo check --workspace
cargo run -p kubernetes-mcp
cargo run -p kubernetes-mcp -- --http
cargo run -p investigator -- crd
cargo run -p investigator
cargo run -p investigator-cli -- --help
cargo run -p investigator-alerts -- --help
```

Build the controller, agent, or server image from the repository root:

```sh
docker build -f docker/Dockerfile.app --build-arg PACKAGE=investigator \
  --build-arg BINARY=investigator -t investigator:dev .
docker build -f docker/Dockerfile.agent -t investigator-agent:dev .
docker build -f docker/Dockerfile.server \
  --build-arg PACKAGE=kubernetes-mcp --build-arg BINARY=kubernetes-mcp \
  -t kubernetes-mcp:dev .
```

The reusable app image runs the selected binary; server images start with
`--http`.

## Interactive investigations

### Install the CLI

`investigator-cli` is distributed as a statically linked Linux binary and a
native macOS binary for x86-64 and ARM64. The static Linux builds do not depend
on the host's glibc version. Releases also contain a Windows x86-64 artifact. Install the
newest GitHub release whose tag matches `investigator-cli-v*` into
`~/.local/bin` with:

```sh
curl -fsSL https://raw.githubusercontent.com/arkadiuszspiewak/investigator/main/install.sh | sh
```

Install a specific CLI release or choose another directory:

```sh
curl -fsSL https://raw.githubusercontent.com/arkadiuszspiewak/investigator/main/install.sh | \
  sh -s -- --version 0.1.0

./install.sh --version 0.1.0 --install-dir /usr/local/bin
```

For the default `latest` selection, the installer queries GitHub releases and
ignores chart, controller, server, and other unrelated releases. It downloads
the matching `investigator-cli-v<VERSION>` artifact and verifies it against the
published SHA-256 checksums. It accepts a complete tag such as
`--version investigator-cli-v0.1.0` as well.
Review [`install.sh`](install.sh) before piping it to a shell if required by your
security policy. To build locally instead, run
`cargo install --path apps/investigator-cli`.

### Configure and run

The CLI reads connection and Investigation defaults from
`~/.investigator-cli/config.json`, or from a path supplied with `--config`. See
[`deploy/examples/investigator-cli-config.json`](deploy/examples/investigator-cli-config.json)
for the complete shape. The file contains Kubernetes Secret references, not the
secret values themselves.

Start an active session. The CLI asks for the initial question and then stays in
a short `follow-up>` loop; no configuration flags are repeated between turns:

```sh
investigator-cli --run-investigation checkout-latency
investigator-cli --run-investigation checkout-latency --config ./config.json
```

For scripting or a single answer, use one-shot mode. Its Investigation name is
generated unless `--name` is supplied:

```sh
investigator-cli --one-shot "Why is checkout slow?"
investigator-cli --one-shot "Audit the payments namespace" --name payments-audit --config ./config.json
```

The CLI creates an Investigation and appends session follow-ups to
`spec.questions`. The controller creates one immutable Job per turn and
correlates responses in `status.answers`, so the conversation remains visible
through the Kubernetes API. The user's kube identity needs `get`, `create`, and
`patch` on investigations; one-shot only requires `get` and `create`.

## Alert-driven investigations

`investigator-alerts` accepts Alertmanager webhook payloads at `POST /alerts`,
creates one Investigation per firing alert fingerprint, and optionally posts the
short result to a Slack incoming webhook. Enable `apps.alerts` in the chart,
configure exactly one Investigation credential reference and MCP endpoint list,
then point an Alertmanager webhook receiver at
`http://<release>-investigator-platform-alerts:8080/alerts`.

The delivery boundary is intentionally small: Alertmanager ingestion creates
CRs, the core controller executes them, and notification delivery is isolated in
the alert app. Additional chat providers can be added there without changing the
CRD or controller.

## Investigation flow

1. Install `charts/investigator-platform`; it contains the Investigation CRD,
   controller, and enabled MCP server Deployments and Services.
2. Create an Investigation like `deploy/examples/investigation.yaml`.
3. The controller creates an owned Job running `codex exec --full-auto <query>`.
4. The agent receives MCP endpoints as `INVESTIGATOR_MCP_SERVERS`; its image
   must translate that JSON into Codex configuration.

Create a Secret for one of the two supported Codex credential modes, then
reference it from `spec.auth`:

```sh
kubectl -n investigations create secret generic openai-api-key \
  --from-literal=api-key="$OPENAI_API_KEY"

kubectl -n investigations create secret generic codex-auth \
  --from-file=auth.json="$HOME/.codex/auth.json"
```

Use either `auth.apiKeySecretRef` or `auth.authJsonSecretRef`, never both. API
keys are exposed to the runner as `OPENAI_API_KEY`. An `auth.json` is copied
into an ephemeral, writable `CODEX_HOME`; the Kubernetes Secret remains
read-only.

The ServiceAccount named by `spec.serviceAccountName` is the investigation's
security boundary. Do not use the controller ServiceAccount for agent Jobs.

See [architecture](docs/architecture.md) for design decisions and next steps.
See [adding an MCP server](docs/adding-mcp-server.md) for the extension checklist.
See [Prometheus MCP server](docs/prometheus-mcp.md) for its tools and configuration.
See [Alertmanager MCP server](docs/alertmanager-mcp.md) for alert investigation tools.

## Helm and releases

```sh
helm upgrade --install investigator charts/investigator-platform \
  --namespace investigations --create-namespace \
  --set investigator.image.repository=ghcr.io/my-org/investigator \
  --set mcpServers.kubernetes.image.repository=ghcr.io/my-org/kubernetes-mcp
```

Each provider has dedicated configuration under `mcpServers.<provider>`. Copy
an entry and set `enabled: true` to add one without changing templates. Server
images use thin path-filtered workflows that call the shared
`_build-server.yml`; chart tags such as `chart-v0.2.0` publish Helm releases.

Tags named `investigator-cli-v<VERSION>` build native CLI archives for supported
platforms, publish checksums, and create a GitHub release consumed by
`install.sh`. The CLI is not packaged as a container image.

When upgrading from an operator-backed chart release, delete the old
`MCPServer` resources and wait for their owned Deployments and Services to be
removed before upgrading. The chart deliberately retains the same Service
names so existing Investigation URLs continue to work:

```sh
kubectl delete mcpservers.mcp.x-k8s.io \
  --namespace investigations \
  --selector app.kubernetes.io/instance=investigator \
  --wait
```
