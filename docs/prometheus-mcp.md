# Prometheus MCP server

`prometheus-mcp` is a small, read-only MCP server backed by the Prometheus HTTP
API. It deliberately avoids alert mutation, rule mutation, and administrative
endpoints.

## Tools

- `query_promql`: execute an instant PromQL query.
- `query_promql_range`: execute PromQL over a time range.
- `list_metric_names`: discover metric names, optionally using series matchers.
- `list_label_values`: discover values for a label such as `namespace` or `job`.
- `list_targets`: inspect active or dropped scrape targets and target health.

## Configuration

- `PROMETHEUS_URL` is required, for example
  `http://prometheus-operated.monitoring.svc.cluster.local:9090`.
- `PROMETHEUS_TIMEOUT_SECONDS` defaults to `30`.
- `PROMETHEUS_BEARER_TOKEN` optionally adds bearer authentication. In Helm,
  inject it with `valueFrom.secretKeyRef`; do not put tokens directly in values.

Enable the chart entry with `mcpServers.prometheus.enabled=true`. The server
does not require Kubernetes RBAC because it talks only to Prometheus.
