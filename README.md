# datadog-query-cli

Datadog API query CLI in Rust for local AI-assisted/debug workflows where MCP is unavailable.

## Features

- Query logs (`/api/v2/logs/events/search`)
- Query metrics (`/api/v1/query`)
- Query events (`/api/v2/events`)
- Call any Datadog endpoint with `raw`
- Uses standard Datadog API key + app key auth

## Why this tool

- Avoid MCP allowlist blockers
- Run fast ad-hoc Datadog queries from terminal
- Keep one CLI for logs, metrics, events, and arbitrary API calls

## Requirements

- Rust toolchain
- Datadog API credentials:
  - `DD_API_KEY`
  - `DD_APP_KEY` (or `DD_APPLICATION_KEY`)
- Optional:
  - `DD_SITE` (defaults to `datadoghq.com`)

## Install / Build

```bash
cargo build
```

Binary path after build:

```bash
./target/debug/datadog-query-cli --help
```

Local release install to `~/.local/bin`:

```bash
cargo build --release --locked
install -Dm755 target/release/datadog-query-cli ~/.local/bin/datadog-query-cli
```

Optional short alias:

```bash
ln -sf ~/.local/bin/datadog-query-cli ~/.local/bin/ddq
```

## Quickstart

```bash
export DD_API_KEY=...
export DD_APP_KEY=...
export DD_SITE=datadoghq.com

cargo run -- logs "service:api @http.status_code:[500 TO 599]" --from now-30m --to now --limit 20
```

If your org is on a different Datadog site, set `DD_SITE` accordingly (for example `us3.datadoghq.com`).

## Usage

Global flags:

- `--site`: Datadog site or full API URL
- `--api-key`: override `DD_API_KEY`
- `--app-key`: override `DD_APP_KEY`/`DD_APPLICATION_KEY`
- `--compact`: print compact JSON

### Logs query

```bash
cargo run -- logs "env:prod service:web" --from now-1h --to now --limit 50 --sort desc
```

### Metrics query

```bash
cargo run -- metrics "avg:system.cpu.user{host:my-host}" --from now-15m --to now
```

`--from` and `--to` for metrics accept:

- unix seconds (`1739990000`)
- RFC3339 (`2026-02-19T17:30:00Z`)
- relative (`now`, `now-15m`, `now-2h`, `now-1d`)

### Events query

```bash
cargo run -- events --query "service:web status:error" --from now-1h --to now --limit 25
```

### Query examples used in practice

Recent errors:

```bash
cargo run -- logs "status:error OR @http.status_code:[500 TO 599] OR level:error OR severity:error" --from now-1h --to now --limit 20
```

Text search:

```bash
cargo run -- logs "\"fix loop exausted\" OR \"fix loop exhausted\"" --from now-7d --to now --limit 20
```

### Raw API query

```bash
cargo run -- raw \
  --method GET \
  --path /api/v1/validate
```

With query params:

```bash
cargo run -- raw \
  --method GET \
  --path /api/v1/monitor \
  --query page=0 \
  --query per_page=10
```

With JSON body:

```bash
cargo run -- raw \
  --method POST \
  --path /api/v2/logs/events/search \
  --body '{"filter":{"query":"service:api","from":"now-15m","to":"now"},"page":{"limit":10}}'
```

## Site examples

- `datadoghq.com` -> `https://api.datadoghq.com`
- `us3.datadoghq.com` -> `https://api.us3.datadoghq.com`
- `datadoghq.eu` -> `https://api.datadoghq.eu`
- `https://api.us5.datadoghq.com` (full URL accepted)

## Troubleshooting

- `403 Forbidden` on `/api/v1/validate`:
  - Most often `DD_SITE` is wrong for your API/app keys.
  - Try another site (for example `us3.datadoghq.com`) and re-run:

```bash
cargo run -- --site us3.datadoghq.com raw --method GET --path /api/v1/validate
```

## GitHub Releases

The repo includes `.github/workflows/release.yml`:

- Push a tag like `v0.1.1` to build and publish release assets
- Or run the workflow manually and provide a `tag`
