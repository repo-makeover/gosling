---
title: Goose v1.47 compatibility (historical)
description: Historical gosling v1.1.0 import notes for Git branch selection, registered OAuth clients, and recent models.
---

# Goose v1.47 compatibility

:::info Historical compatibility record
This page preserves the 2026-08-23 import snapshot for gosling `v1.1.0`. The
[current feature comparison](goose-comparison.md) records the versions and date
of the latest source review.
:::

This guide compares Goose `v1.47.0` with gosling `v1.1.0` for three imported
features: a Git branch indicator, pre-registered OAuth
clients for Streamable HTTP MCP extensions, and recently used models in the
Desktop picker. The implementations were adapted from the Goose source and use
gosling’s existing Electron, ACP, configuration, and RMCP 1.7 seams. They aim
for compatible user behavior, not byte-for-byte or protocol-identical parity.

## Implemented compatibility

| Goose v1.47 feature | gosling v1.1.0 behavior | Important boundary |
|---|---|---|
| Git branch indicator | The Desktop chat footer shows the current branch for the selected working directory. Its menu lists local branches and can switch to one. | Every Git IPC call first applies gosling’s renderer directory-grant check. Remote branch management, worktree creation, and merge operations are out of scope. |
| Pre-registered OAuth clients for Streamable HTTP MCP | `streamable_http` extension configuration accepts `client_id`, `client_secret_key`, and `scopes`. A registered client is used for stored-token refresh and interactive authorization. | gosling uses its installed RMCP 1.7 API, so its internal OAuth flow is adapted rather than identical to Goose’s newer implementation. |
| Recently used models | The Desktop model menu persists and displays the five most recent successful model/provider selections, excluding the active model. | This is a Desktop preference. It does not change CLI model selection or provider defaults. |

## Configuring a registered OAuth client

Use a Streamable HTTP extension entry in the normal `extensions` section. The
client secret is named by `client_secret_key`; its value must come from the
extension environment or gosling’s secret store. Do not write the secret itself
into the extension configuration.

```yaml
extensions:
  private-mcp:
    enabled: true
    name: private-mcp
    type: streamable_http
    uri: https://mcp.example.com/mcp
    client_id: ${MCP_CLIENT_ID}
    client_secret_key: MCP_CLIENT_SECRET
    scopes:
      - tools.read
    env_keys:
      - MCP_CLIENT_ID
      - MCP_CLIENT_SECRET
```

`client_id` supports the same environment-variable substitution as the HTTP URI
and headers. `client_secret_key` is a lookup key, not a substitution expression:
gosling first checks the extension’s merged environment, then its secret store.
An OAuth secret without a `client_id`, or a missing referenced secret, is
rejected as invalid configuration.

The Desktop extension form preserves these fields when editing an existing
extension. Configure the fields in file-based configuration or an ACP client;
the form does not expose a client-secret text box.

## Verification scope

The feature work is covered by Rust configuration/ACP round trips and resolver
tests, plus the Desktop typecheck and test suite. The Git UI uses direct Git
commands only through Electron’s granted-directory boundary. OAuth interaction
with a third-party authorization server is environment-dependent and requires
the server’s real registered client and redirect configuration.
