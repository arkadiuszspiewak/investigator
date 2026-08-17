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

If an Investigation with that name already exists, the CLI resumes it instead:
it displays the stored conversation, waits for any running turn, and opens the
follow-up prompt. It does not replace the original query or create another CR.

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
creates one Investigation per firing episode (fingerprint plus `startsAt`), and optionally posts the
short result through either a Slack incoming webhook or a Slack App. Enable `apps.alerts` in the chart,
configure exactly one Investigation credential reference and MCP endpoint list,
then point an Alertmanager webhook receiver at
`http://<release>-investigator-platform-alerts:8080/alerts`.

For incoming-webhook delivery, provide `SLACK_WEBHOOK_URL` from a Kubernetes
Secret. For Slack App delivery, provide both `SLACK_BOT_TOKEN` (normally an
`xoxb-` token from a Secret) and `SLACK_CHANNEL` (preferably the channel ID).
The app needs the `chat:write` scope and must be invited to the destination
channel; alternatively grant the additional Slack permission appropriate for
posting to channels it has not joined. `SLACK_WEBHOOK_URL` and Slack App
credentials are mutually exclusive. `SLACK_API_URL` can override the default
`https://slack.com/api` endpoint.

By default, alert Investigations are created in the Helm release namespace.
Override this with `apps.alerts.investigationNamespace`; the chart creates the
Role and RoleBinding in that same target namespace. The namespace is a dedicated
value rather than part of `apps.alerts.env`, so replacing the environment list
for credentials cannot accidentally reset it to `default`.

### Slack relay mode

Slack delivery defaults to independent mode (`RELAY_MODE=false`): Alertmanager
can keep sending its native Slack notification, while `investigator-alerts`
posts the completed analysis separately. This is the safest mode when native
Alertmanager delivery must continue working even if the investigation service
is unavailable.

Set `apps.alerts.relayMode=true` (which supplies `RELAY_MODE=true`) with
`SLACK_BOT_TOKEN` and `SLACK_CHANNEL` to let
`investigator-alerts` own the Slack conversation. It immediately posts a compact
parent alert, records Slack's channel and message timestamp on the Investigation,
and posts the completed analysis as a thread reply. Incoming webhooks are not
supported in relay mode because they do not reliably return the parent timestamp.

```yaml
apps:
  alerts:
    enabled: true
    relayMode: true
    env:
      - name: SLACK_BOT_TOKEN
        valueFrom: {secretKeyRef: {name: slack-app, key: bot-token}}
      - {name: SLACK_CHANNEL, value: "C0123456789"}
      # Include the Investigation credential and MCP settings here as well.
```

When relay mode is enabled, remove the matching native Slack route to avoid two
parent alerts. Alternatively, keep the native route intentionally as a fallback;
Slack will then receive both the native alert and the relay-owned threaded alert.

The delivery boundary is intentionally small: Alertmanager ingestion creates
CRs, the core controller executes them, and notification delivery is isolated in
the alert app. Additional chat providers can be added there without changing the
CRD or controller.

## Investigation flow

1. Install `charts/investigator-platform`; it contains the Investigation CRD,
   controller, and enabled MCP server Deployments and Services.
2. Create an Investigation like `deploy/examples/investigation.yaml`.
3. The controller creates an owned Job running `codex exec --full-auto <query>`.
4. Helm builds the controller-owned MCP registry from only the enabled
   `mcpServers` entries. Investigation resources and clients cannot override it.
5. The agent receives those endpoints as `INVESTIGATOR_MCP_SERVERS`;
   its image translates that JSON into Codex configuration.

Agent provider and authentication are global Helm configuration. Investigation
resources contain conversation state only. For OpenAI API-key authentication:

```sh
kubectl -n investigations create secret generic openai-api-key \
  --from-literal=api-key="$OPENAI_API_KEY"

```

Configure `agent.provider`, `agent.auth`, and `agent.serviceAccount`. Supported
modes are OpenAI with `apiKey` or `authJson`, and Bedrock with `apiKey` or
`workloadIdentity`. Workload identity uses EKS Pod Identity or IRSA credentials
to generate region-bound, short-lived Bedrock bearer tokens. Codex uses its
native `amazon-bedrock` provider and refreshes the token during long-running Jobs.

For Bedrock with a stored API key:

```yaml
agent:
  provider: {type: bedrock, model: qwen.qwen3-coder-next, region: eu-central-1, projectId: proj_example}
  auth:
    type: apiKey
    apiKeySecretRef: {name: bedrock-api-key, key: api-key}
```

For Bedrock with EKS Pod Identity or IRSA:

```yaml
agent:
  provider: {type: bedrock, model: qwen.qwen3-coder-next, region: eu-central-1, projectId: proj_example}
  auth: {type: workloadIdentity}
  serviceAccount:
    create: true
    name: investigator-agent
    annotations: {}
```

EKS Pod Identity associations are configured outside the chart. For IRSA, add
`eks.amazonaws.com/role-arn` to `agent.serviceAccount.annotations`. The role
must be allowed to invoke the selected Bedrock model through the Mantle endpoint.
The required `projectId` is sent as the `OpenAI-Project` header on every request,
so usage is isolated in the tagged Bedrock Mantle project instead of `default`.

Agent Job placement is cluster policy and is configured once in Helm, rather
than on individual Investigation resources. For example:

```yaml
agent:
  job:
    nodeSelector: {}
    tolerations: []
    affinity:
      nodeAffinity:
        requiredDuringSchedulingIgnoredDuringExecution:
          nodeSelectorTerms:
            - matchExpressions:
                - key: rpi
                  operator: NotIn
                  values: ["true"]
            - matchExpressions:
                - key: rpi
                  operator: DoesNotExist
```

The controller applies `agent.job.nodeSelector`, `affinity`, and
`tolerations` to every newly created investigation Job. Changing these settings
does not mutate Jobs that Kubernetes has already created.

See [architecture](docs/architecture.md) for design decisions and next steps.
See [adding an MCP server](docs/adding-mcp-server.md) for the extension checklist.
See [Prometheus MCP server](docs/prometheus-mcp.md) for its tools and configuration.
See [Alertmanager MCP server](docs/alertmanager-mcp.md) for alert investigation tools.

## Helm and releases

```sh
helm upgrade --install investigator charts/investigator-platform \
  --namespace investigations --create-namespace \
  --set investigator.image.repository=ghcr.io/my-org/investigator \
  --set agent.image.tag=0.1.0 \
  --set mcpServers.kubernetes.image.repository=ghcr.io/my-org/kubernetes-mcp
```

`agent.image` configures the Codex agent image used by every
Investigation Job. Individual Investigation resources cannot override it.

Each provider has dedicated configuration under `mcpServers.<provider>`. Copy
an entry and set `enabled: true` to add one without changing templates. Server
images use thin path-filtered workflows that call the shared
`_build-server.yml`.

Enabled providers automatically become available to every Investigation.
Server URLs are generated centrally from the Helm release namespace, Service
name, port, and MCP path. Investigation resources, the CLI, and the alerts app
do not accept MCP endpoints, preventing clients from injecting or omitting
servers.

Release Please manages independent versions and GitHub Releases for every app,
server, the agent, and the chart. Commits on `main` must use Conventional Commit
prefixes: `fix:` creates a patch release, `feat:` creates a minor release, and a
type followed by `!` (for example, `feat!:`) creates a major release. Commits
without a release-bearing prefix do not increment a version.

Release Please maintains one combined release PR containing only components
with release-bearing changes. Merging that PR creates component tags such as
`investigator-v0.2.0` and `chart-v0.2.0`, creates the corresponding GitHub
Releases, and dispatches the existing artifact workflow for each released
component. Tags named `investigator-cli-v<VERSION>` build native CLI archives
for supported platforms and attach them and their checksums to the GitHub
Release consumed by `install.sh`. The CLI is not packaged as a container image.

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
