# Alertmanager MCP server

`alertmanager-mcp` is a small, read-only MCP server backed by Alertmanager's v2
HTTP API. It intentionally does not expose alert submission, silence creation or
deletion, configuration reloads, or other mutations.

## Tools

- `list_active_alerts`: list active alerts, with optional label, receiver, and
  suppression-state filters.
- `list_alert_groups`: view active alerts grouped as Alertmanager routes them.
- `list_silences`: inspect silences and their current state.
- `list_receivers`: discover configured receiver names.
- `get_status`: inspect cluster, version, and configuration status.

Label filters use Alertmanager matcher syntax, such as `severity="critical"` or
`namespace=~"prod-.*"`. Multiple filters are combined by Alertmanager.

## Configuration

- `ALERTMANAGER_URL` is required, for example
  `http://alertmanager-operated.monitoring.svc.cluster.local:9093`.
- `ALERTMANAGER_TIMEOUT_SECONDS` defaults to `30`.
- `ALERTMANAGER_BEARER_TOKEN` optionally adds bearer authentication. In Helm,
  inject it with `valueFrom.secretKeyRef`; do not put tokens directly in values.

Enable the chart entry with `mcpServers.alertmanager.enabled=true`. The server
does not require Kubernetes RBAC because it only makes read-only HTTP requests
to Alertmanager.
