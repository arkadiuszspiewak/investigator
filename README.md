# Investigator

A Rust workspace for an in-cluster Codex investigator and the MCP servers it
uses. MCP servers expose focused capabilities; the `investigator` controller
turns `Investigation` resources into short-lived Codex agent Jobs.

## Layout

```text
apps/investigator/          Investigation CRD and Job controller
crates/mcp-runtime/         shared MCP Streamable HTTP transport
servers/kubernetes-mcp/    read-only Kubernetes MCP server and tools
charts/investigator-platform/ central controller + MCP server chart
docker/                     controller and reusable server Dockerfiles
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
```

Build the controller, agent, or server image from the repository root:

```sh
docker build -f docker/Dockerfile.investigator -t investigator:dev .
docker build -f docker/Dockerfile.agent -t investigator-agent:dev .
docker build -f docker/Dockerfile.server \
  --build-arg PACKAGE=kubernetes-mcp --build-arg BINARY=kubernetes-mcp \
  -t kubernetes-mcp:dev .
```

The server image starts with `--http`; the investigator image starts the
controller with no arguments.

## Investigation flow

1. Install the MCP Lifecycle Operator.
2. Install `charts/investigator-platform`; it contains the Investigation CRD,
   controller, and enabled MCP servers.
3. Create an Investigation like `deploy/examples/investigation.yaml`.
4. The controller creates an owned Job running `codex exec --full-auto <query>`.
5. The agent receives MCP endpoints as `INVESTIGATOR_MCP_SERVERS`; its image
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
