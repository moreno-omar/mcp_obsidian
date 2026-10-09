# DEPLOYMENT.md — hosting on a VPS, then listing on Smithery and ChatGPT

Goal: put the MCP server behind a public HTTPS URL so a remote host can reach it,
then submit that URL to Smithery and the ChatGPT/plugin directory.

Crates.io is already done (`cargo install mcp-obsidian`). The Official MCP
Registry is a separate, simpler path and is **not** covered here — see
[PUBLISHING.md](PUBLISHING.md#2-official-mcp-registry--deferred).

> **Read this first.** `serve --http` has **no authentication**. Anyone who can
> reach the URL can read every note in the vault. Until OAuth exists
> ([step 6](#6-oauth--the-one-real-code-gap)), either keep it loopback-only or put
> HTTP basic auth in front of it at the proxy. Both OpenAI and Smithery's URL tab
> require auth for a listing anyway.

## What you need

| # | Thing | Notes |
| :- | :- | :- |
| 1 | A VPS with a public IP | 1 vCPU / 1 GB is plenty; retrieval is CPU-light and the vault is in memory |
| 2 | A domain or subdomain | e.g. `notes.example.com`; needed for TLS and for OpenAI's domain check |
| 3 | The vault **on the VPS** | retrieval reads local disk — it is not a remote view of your desktop vault |
| 4 | TLS | Caddy gets a Let's Encrypt cert automatically; nginx + certbot also fine |
| 5 | OAuth | **required by OpenAI**, optional for Smithery. Not implemented yet — [step 6](#6-oauth--the-one-real-code-gap) |

## 1. DNS and firewall

Point the record at the VPS, then confirm it resolves before touching TLS:

```bash
dig +short notes.example.com          # must print the VPS IP
sudo ufw allow 80,443/tcp             # TLS + the ACME challenge
sudo ufw enable
```

Do **not** open the app's port. The server binds loopback and only the proxy
talks to it.

## 2. Install the server

```bash
# either from crates.io
cargo install mcp-obsidian
# or grab a prebuilt binary from the GitHub releases page
```

Put the vault somewhere the service account can read:

```bash
sudo install -d -o root -g mcpobsidian -m 0750 /srv/vault
# then sync it: rsync -a --delete /local/vault/ vps:/srv/vault/
```

The server opens the vault **read-only** and never writes to it, so a one-way
sync is safe and a read-only mount is even better.

## 3. Run it under systemd

A dedicated unprivileged account, no shell, and a locked-down unit:

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin mcpobsidian
```

`/etc/systemd/system/mcp-obsidian.service`:

```ini
[Unit]
Description=mcp-obsidian MCP server (stdio-free, Streamable HTTP)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=mcpobsidian
Group=mcpobsidian
ExecStart=/usr/local/bin/mcp-obsidian \
  --vault /srv/vault \
  serve \
  --http 127.0.0.1:8000 \
  --allow-host notes.example.com
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

# The vault is read-only input: enforce that at the sandbox level too.
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_INET AF_INET6
ReadOnlyPaths=/srv/vault

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mcp-obsidian
sudo systemctl status mcp-obsidian --no-pager
journalctl -u mcp-obsidian -n 30 --no-pager
```

**`--allow-host` is not optional.** `rmcp` rejects any `Host` header that is not
loopback (its DNS-rebinding guard), so without the public name every request
through the proxy returns `403 Forbidden: Host header is not allowed`. The value
must match exactly what the proxy forwards — `$host` without the port, or
`$host:$server_port` if you forward the port.

Sanity-check on the box before adding TLS:

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:8000/mcp \
  -H 'Host: notes.example.com' \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"1"}}}'
# 200 = good. 403 = --allow-host does not match the Host header you sent.
```

## 4. Terminate TLS

The MCP endpoint is `https://notes.example.com/mcp`. Streamable HTTP keeps a
response open for server-sent events, so **buffering must be off** and timeouts
long, or the client hangs and the proxy eventually 502s.

<details open>
<summary><b>Caddy</b> — automatic certs, buffering off by default</summary>

`/etc/caddy/Caddyfile`:

```caddyfile
notes.example.com {
    reverse_proxy 127.0.0.1:8000 {
        # keep the app's Host guard satisfied with the public name
        header_up Host {host}
        # stream, do not buffer (required for SSE)
        flush_interval -1
        transport http {
            read_timeout 0
            write_timeout 0
        }
    }
}
```

```bash
sudo systemctl reload caddy
```
</details>

<details>
<summary><b>nginx</b> — the three directives that matter</summary>

```nginx
server {
    listen 443 ssl http2;
    server_name notes.example.com;

    # certbot fills these in
    ssl_certificate     /etc/letsencrypt/live/notes.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/notes.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;          # must match --allow-host
        proxy_set_header Connection "";

        proxy_buffering off;                  # required for SSE
        proxy_cache off;
        proxy_read_timeout 3600s;             # long-lived event streams
        proxy_send_timeout 3600s;
        chunked_transfer_encoding on;
    }
}
```

```bash
sudo certbot --nginx -d notes.example.com
sudo nginx -t && sudo systemctl reload nginx
```
</details>

## 5. Verify the public URL

This is the same handshake a host performs — the session id in the response header
is the proof the transport works end to end:

```bash
curl -sS -D- -o /dev/null -X POST https://notes.example.com/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"1"}}}'
```

Expect `200 OK`, `content-type: text/event-stream` and an `mcp-session-id`
header. Then drive a real tool call:

```bash
SID=$(curl -sS -D- -o /dev/null -X POST https://notes.example.com/mcp \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"1"}}}' \
  | tr -d '\r' | awk -F': ' '/^mcp-session-id/{print $2}')

curl -sS -X POST https://notes.example.com/mcp \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -H "mcp-session-id: $SID" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
```

Common failures, in order of likelihood:

| Symptom | Cause |
| :- | :- |
| `403 Host header is not allowed` | `--allow-host` does not match the proxy's `Host` |
| `502` / hangs after a while | proxy buffering on, or a short `proxy_read_timeout` |
| `curl` works, real client does not | `Accept` header missing `text/event-stream` |
| Works over `http://`, fails over `https` | proxy not forwarding to loopback, or cert not trusted |

## 6. OAuth — the one real code gap

**Both** OpenAI's review and Smithery's URL flow expect authentication. The server
does not have any yet, so this has to be built before either listing is
submittable. It is a new phase, not a config change.

What is required, in the vocabulary both reviewers use:

1. **An OAuth 2.1 authorization server** for users to grant access, advertised via
   `/.well-known/oauth-authorization-server`.
2. **Protected Resource Metadata** at
   `/.well-known/oauth-protected-resource`, pointing at that authorization server.
3. **Client registration** — one of: a client ID/secret you configure in the
   OpenAI portal, **Dynamic Client Registration** (a `registration_endpoint`), or
   **Client ID Metadata Documents** (CIMD).
4. **Token validation on every `/mcp` request** — a `401` with a
   `WWW-Authenticate: Bearer` header pointing at the metadata document.

`rmcp` ships an `auth` feature (`rmcp = { features = ["auth"] }`) with the
server-side pieces, so this is wiring plus policy, not a protocol implementation.
Until it lands, the honest interim state is **basic auth at the proxy** and no
public listing — which is also why the OpenAI work is sequential, not parallel.

## 7. Smithery

Two routes; you already have everything for the second one.

### Route A — the public URL (what this document sets up)

1. Sign in at <https://smithery.ai/new> with GitHub.
2. Choose the **URL** tab and paste `https://notes.example.com/mcp`.
3. Supply a config schema if the host should prompt for anything. Here the vault
   path is fixed server-side, so there is nothing for a user to configure — pass
   an empty schema rather than exposing the filesystem path as an input.
4. Smithery expects Streamable HTTP, with OAuth if the endpoint is authenticated
   — which is the state you will want anyway for [step 6](#6-oauth--the-one-real-code-gap).

### Route B — the MCPB bundle (no VPS, no OAuth)

The bundle already builds and is schema-validated, so this is the faster listing
and needs no hosting at all:

```bash
./scripts/build_mcpb.sh          # → dist/mcp-obsidian-0.1.0-linux.mcpb
```

Upload that file at <https://smithery.ai/new>. It carries a native binary, so
build one per platform you want to support (macOS and Windows need a cross-build
matrix, and signing if you want them to install without warnings).

## 8. ChatGPT / plugin directory

Reach this only after [step 6](#6-oauth--the-one-real-code-gap) is deployed and
verified — every item below is checked by a reviewer, and the one-version-in-review
rule means a rejection costs a full cycle.

### Requirements checklist

- [ ] **Public HTTPS endpoint** on a real domain. Local and test endpoints are
      rejected outright; `https://notes.example.com/mcp` qualifies.
- [ ] **Streamable HTTP** transport — this is why `serve --http` exists.
- [ ] **OAuth** with a supported registration method (see step 6).
- [ ] **Domain verification** — the portal gives you a token; serve it as
      **plain text** (not JSON, not a list) at:
      `https://notes.example.com/.well-known/openai-apps-challenge`
      The host must be the MCP hostname or an eligible parent domain. With Caddy
      that is a two-line route; with nginx a `location =` returning the token with
      `Content-Type: text/plain`, and it must be exempt from the OAuth gate.
- [ ] **Verified OpenAI Platform identity** — individual or business, in the
      dashboard. Publishing under an unverified name is rejected.
- [ ] **Public Terms of Service and privacy policy.** This matters more than
      usual here: the app reads the *user's* notes, so the policy has to say what
      is read, that it stays on the server, and what is retained.
- [ ] **No pass-through apps** — the directory does not accept unofficial
      connectors.

### Review package

- [ ] **5 positive test cases** — prompts that must work, e.g. a tag lookup, a
      full-text term lookup, a wikilink-graph walk, reading one note, and a
      "nothing matches" case that must answer honestly rather than invent.
- [ ] **3 negative test cases** — e.g. an unknown note, a malformed argument, and
      a question the vault has no material for.
- [ ] **Reviewer credentials** — a working login with **no MFA and no magic-link**,
      documented in the submission. Budget for this: reviewers will not do a
      device-code dance.
- [ ] **A video walkthrough** of the app in use.
- [ ] **One version in review at a time.**

Submit the ZIP at <https://platform.openai.com/plugins>. Automated metadata and
tool scans run first, so the four tool descriptions in `src/mcp_server.rs` are
already the copy the reviewer reads — keep them accurate.

## Order to do it in

1. Steps 1–5 — get the URL answering `initialize`. This is verifiable today.
2. **Decide on the hosting trade-off.** The vault now lives on a public server;
   that is a larger change than it looks, and OAuth is what makes it defensible.
3. Step 6 — build and deploy OAuth.
4. Step 7 — Smithery. Fast feedback, and it exercises the URL with a real client.
5. Step 8 — OpenAI. Last, because it is the most expensive to get wrong.

## References

- OpenAI plugin review requirements: <https://developers.openai.com/plugins/deploy/app-review>
- OpenAI plugin submission flow: <https://developers.openai.com/plugins/deploy/submission>
- Smithery publishing: <https://smithery.ai/docs/build/publish>
- MCP Streamable HTTP transport: <https://modelcontextprotocol.io/specification/2025-06-18/basic/transports>
- MCP Authorization: <https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization>
