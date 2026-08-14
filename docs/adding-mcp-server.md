# Adding an MCP server

1. Add `servers/<name>-mcp` to the Cargo workspace. Reuse `mcp-runtime` when it
   uses the standard Streamable HTTP transport.
2. Add `mcpServers.<name>` to the chart values. Keep it disabled until its image
   and provider configuration are ready. RBAC belongs to that entry, not to a
   shared server role.
3. Add `.github/workflows/build-<name>-mcp.yml` as a path-filtered caller:

```yaml
name: Build <name>-mcp
on:
  pull_request:
    paths: [servers/<name>-mcp/**, crates/mcp-runtime/**, Cargo.toml, Cargo.lock]
  push:
    branches: [main]
    paths: [servers/<name>-mcp/**, crates/mcp-runtime/**, Cargo.toml, Cargo.lock]
  workflow_dispatch:
jobs:
  image:
    uses: ./.github/workflows/_build-server.yml
    permissions:
      contents: read
      packages: write
    with:
      package: <name>-mcp
      binary: <name>-mcp
      image: <name>-mcp
    secrets: inherit
```

4. Render all enabled chart branches before release:

```sh
helm lint charts/investigator-platform
helm template test charts/investigator-platform \
  --set mcpServers.<name>.enabled=true
```

The reusable workflow owns Docker tags, GHCR authentication, and build cache.
Server callers own only triggers and package/image identity.
