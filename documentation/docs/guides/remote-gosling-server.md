---
sidebar_position: 90
title: Running a Separate Local gosling Server
sidebar_label: Local External Server
---

# Running a Separate Local gosling Server

:::caution Needs verification against the current `gosling serve` command
This guide describes the deployment pattern (run the ACP server separately, point Desktop at it) using the old standalone `goslingd` binary, which no longer exists — `crates/gosling-server` was removed because nothing in this workspace built or shipped it anymore. The equivalent functionality is now the CLI's own `gosling serve` subcommand (see `crates/gosling-cli/src/cli.rs`, `Command::Serve`), which confirmed differences include:
- No `GOSLING_HOST`/`GOSLING_PORT` env vars — host/port are `--host`/`--port` flags (defaults `127.0.0.1` / `3284`, not port 3000).
- No `agent` sub-verb — `gosling serve` is the complete command.
- `GOSLING_TLS`/`GOSLING_TLS_CERT_PATH`/`GOSLING_TLS_KEY_PATH` and `GOSLING_SERVER__SECRET_KEY` still appear to be read the same way, but this has not been independently re-verified end-to-end.
- Whether the self-signed certificate + `GOSLINGD_CERT_FINGERPRINT` log line and the Settings → gosling Server UI fields below still exist as described has **not** been re-checked.

The desktop app still has live code that supports pointing it at an external server (`ExternalGoslingdConfig` in `ui/desktop/src/utils/settings` and `ui/desktop/src/utils/csp.ts`), so this is not a dead feature — the content below is kept as a starting point, but treat every command and field name as unverified until someone walks through it against the current build.
:::

gosling Desktop normally starts its own backend server process. Advanced local setups may start that process separately and connect Desktop to it through a loopback address.

The server is a single-operator local control plane. It does not support binding to a LAN, VPN, public, wildcard, or other non-loopback address without additional authentication. Use a separately designed multi-user service instead of exposing it remotely.

## Initial Setup

### 1. Start the server

On the same machine as Desktop, launch the server with a loopback host, port, TLS, and a secret key. With the old `goslingd` binary this was:

```bash
GOSLING_HOST=127.0.0.1 \
GOSLING_PORT=3000 \
GOSLING_TLS=true \
GOSLING_SERVER__SECRET_KEY='YOUR_SECRET' \
/Applications/Gosling.app/Contents/Resources/bin/goslingd agent
```

The current equivalent is the `gosling` CLI's `serve` subcommand, e.g.:

```bash
GOSLING_TLS=true \
GOSLING_SERVER__SECRET_KEY='YOUR_SECRET' \
/Applications/Gosling.app/Contents/Resources/bin/gosling serve --host 127.0.0.1 --port 3000
```

This has not been run end-to-end against the current build — verify it starts and listens as expected before relying on it.

| Variable / flag | Purpose |
|----------|---------|
| `--host` | Loopback address to bind. `--dangerously-unauthenticated` additionally requires this to resolve to loopback. |
| `--port` | TCP port to listen on. |
| `GOSLING_TLS` | Enables TLS. Confirm gosling Desktop still refuses plain HTTP before relying on this. |
| `GOSLING_SERVER__SECRET_KEY` | Shared secret. The client must send this in the `X-Secret-Key` header. Treat it like a password. |

:::tip
Pick a long, random value for `GOSLING_SERVER__SECRET_KEY` and store it in a password manager — the same value goes into gosling Desktop later.
:::

### 2. Verify the server is up

First, confirm the server is actually listening on the port you expect:

```bash
lsof -nP -iTCP:3000 -sTCP:LISTEN
```

Then test the endpoints from the server itself. The `-k` flag tells `curl` to accept the self-signed TLS certificate the server generates (unverified against the current build):

```bash
# Connectivity only
curl -i https://127.0.0.1:3000/status -k

# Authenticated endpoint (real test)
curl -i https://127.0.0.1:3000/config/read -k \
  -H 'Content-Type: application/json' \
  -H 'X-Secret-Key: YOUR_SECRET' \
  --data '{"key":"GOSLING_PROVIDER","is_secret":false}'
```

A `200` response from the second call confirms that TLS is up, the secret key is being accepted, and the server is ready to receive client requests.

### 3. Find the certificate fingerprint

The old `goslingd` generated a self-signed TLS certificate and gosling Desktop pinned it by SHA-256 fingerprint rather than relying on a public certificate authority, logging a `GOSLINGD_CERT_FINGERPRINT=...` line on startup. Whether `gosling serve` still does this has not been re-verified — check the current startup log output before relying on this step.

### 4. Configure gosling Desktop

On the client machine, open gosling Desktop and check whether **Settings → gosling Server** still exposes:

| Setting | Value |
|---------|-------|
| **Use external server** | Enabled |
| **URL** | `https://127.0.0.1:3000` |
| **Secret Key** | The same value you used for `GOSLING_SERVER__SECRET_KEY` |
| **Certificate Fingerprint** | The fingerprint value from the server logs, if the server still logs one |

## Troubleshooting

### Client cannot authenticate (401 / Unauthorized)

A `401` from the server, or a gosling Desktop error indicating that the secret was rejected, almost always means that `GOSLING_SERVER__SECRET_KEY` on the server does not match the **Secret Key** in gosling Desktop's settings.

To check the secret end-to-end without involving gosling Desktop, run the authenticated `curl` from [step 2](#2-verify-the-server-is-up) using exactly the value you have configured on the client. If that returns `200`, the secret is correct and the problem is in the client configuration; if it returns `401`, the secret on the server is different from what you are sending.

If you rotate the secret on the server, you must also update it in gosling Desktop's settings — they are not synchronized automatically.

## Related

- [Environment Variables](/docs/guides/environment-variables) — full reference for all `GOSLING_*` variables
- [Configuration Files](/docs/guides/config-files) — persistent client-side configuration
