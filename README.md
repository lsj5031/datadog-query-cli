# datadog-query-cli

Rust CLI for querying Datadog APIs from terminals and automation workflows.

## Quickstart

```bash
export DD_API_KEY=...
export DD_APP_KEY=...
export DD_SITE=datadoghq.com

datadog-query-cli logs "service:api @http.status_code:[500 TO 599]" \
  --from now-30m --to now --limit 20
```

## Install

### Prebuilt binary (Linux/macOS)

```bash
VERSION="v0.3.0"
REPO="lsj5031/datadog-query-cli"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) TARGET="x86_64-unknown-linux-musl" ;;
  Linux-aarch64|Linux-arm64) TARGET="aarch64-unknown-linux-musl" ;;
  Darwin-x86_64) TARGET="x86_64-apple-darwin" ;;
  Darwin-arm64) TARGET="aarch64-apple-darwin" ;;
  *) echo "Unsupported platform"; exit 1 ;;
esac

ASSET="datadog-query-cli-${VERSION}-${TARGET}"
curl -fsSL -o "/tmp/${ASSET}.tar.gz" \
  "https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}.tar.gz"
curl -fsSL -o "/tmp/checksums.txt" \
  "https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt"

if command -v sha256sum >/dev/null 2>&1; then
  (cd /tmp && grep " ${ASSET}.tar.gz\$" checksums.txt | sha256sum -c -)
else
  (cd /tmp && grep " ${ASSET}.tar.gz\$" checksums.txt > expected.sha256 && shasum -a 256 -c expected.sha256)
fi

tar -xzf "/tmp/${ASSET}.tar.gz" -C /tmp
install -Dm755 "/tmp/${ASSET}" ~/.local/bin/datadog-query-cli
```

### Prebuilt binary (Windows PowerShell)

```powershell
$Version = "v0.3.0"
$Repo = "lsj5031/datadog-query-cli"
$Asset = "datadog-query-cli-$Version-x86_64-pc-windows-msvc.zip"

Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$Version/$Asset" -OutFile "$env:TEMP\$Asset"
Expand-Archive -Path "$env:TEMP\$Asset" -DestinationPath "$env:TEMP\ddq" -Force
New-Item -ItemType Directory -Force "$HOME\bin" | Out-Null
Copy-Item "$env:TEMP\ddq\datadog-query-cli-$Version-x86_64-pc-windows-msvc.exe" "$HOME\bin\datadog-query-cli.exe" -Force
```

### Build from source

```bash
cargo build --release --locked
install -Dm755 target/release/datadog-query-cli ~/.local/bin/datadog-query-cli
```

### Verify signed checksums (cosign keyless)

```bash
VERSION="v0.3.0"
REPO="lsj5031/datadog-query-cli"

curl -fsSL -o checksums.txt \
  "https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt"
curl -fsSL -o checksums.txt.sig \
  "https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt.sig"
curl -fsSL -o checksums.txt.pem \
  "https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt.pem"

cosign verify-blob checksums.txt \
  --signature checksums.txt.sig \
  --certificate checksums.txt.pem \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  --certificate-identity-regexp "https://github.com/.+/.+/.github/workflows/release.yml@.+"
```

## Configuration

Credentials are resolved in priority order:

1. CLI flags (`--api-key`, `--app-key`)
2. Environment variables (`DD_API_KEY`, `DD_APP_KEY` / `DD_APPLICATION_KEY`)
3. TOML config file (`api_key`, `app_key`, `site`)

Config file locations (checked in order):

- `.ddq.toml` (current directory)
- `datadog.toml` (current directory)
- `~/.config/ddq/config.toml` (OS standard)

Example `~/.config/ddq/config.toml`:

```toml
api_key = "..."
app_key = "..."
site = "us3.datadoghq.com"
```

## Usage

Commands:

- `logs`: `/api/v2/logs/events/search`
- `metrics`: `/api/v1/query`
- `events`: `/api/v2/events`
- `raw`: arbitrary Datadog endpoint

Global flags:

- `--site`: Datadog site suffix or full API URL (default from `DD_SITE` or `datadoghq.com`)
- `--api-key`: override `DD_API_KEY`
- `--app-key`: override `DD_APP_KEY`/`DD_APPLICATION_KEY`
- `--output`: `json` (default, compact) or `pretty`
- `--log-level`: `error`, `warn`, `info` (default), `debug`, `trace`
- `--log-file <PATH>`: write JSONL structured logs to file (in addition to stderr)
- `--retries`, `--retry-backoff-ms`, `--retry-max-backoff-ms`, `--retry-rate-limit`, `--retry-timeout-seconds`, `--timeout-seconds`
- `--compact`: deprecated, use `--output json` instead

Examples:

```bash
# Logs
datadog-query-cli logs "env:prod service:web" \
  --from now-1h --to now --limit 50 --sort desc

# Metrics
datadog-query-cli metrics "avg:system.cpu.user{host:my-host}" \
  --from now-15m --to now

# Events
datadog-query-cli events --query "service:web status:error" \
  --from now-1h --to now --limit 25

# Raw GET
datadog-query-cli raw \
  --method GET \
  --path /api/v1/validate

# Raw POST with body
datadog-query-cli raw \
  --method POST \
  --path /api/v2/logs/events/search \
  --body '{"filter":{"query":"service:api","from":"now-15m","to":"now"},"page":{"limit":10}}'
```

### Structured Logging

All logs are JSONL (JSON Lines) to stderr, pipe-friendly for `jq`:

```bash
# Capture stdout, send logs to file
datadog-query-cli --log-level debug --log-file debug.jsonl \
  logs "service:api" --from now-5m 2>/dev/null

# Debug with jq
datadog-query-cli --log-level info logs "service:api" 2>logs.jsonl
cat logs.jsonl | jq 'select(.fields.reason == "rate_limit")'
cat logs.jsonl | jq 'select(.level == "WARN" or .level == "ERROR")'
```

## Error Handling

Success:

- JSON to `stdout`
- exit code `0`

Failure:

- JSON error envelope to `stderr`
- deterministic non-zero exit code

Error envelope format:

```json
{
  "error": {
    "category": "rate_limit",
    "exit_code": 4,
    "status": 429,
    "retryable": true,
    "retried": 3,
    "retry_after_ms": 1000,
    "rate_limit_remaining": 0,
    "rate_limit_limit": 300,
    "rate_limit_reset": 14,
    "rate_limit_period": 30,
    "message": "..."
  }
}
```

Exit codes:

- `1`: internal error
- `2`: usage/config/input error
- `3`: auth error (`401`/`403`)
- `4`: rate-limited (`429`) after retries exhausted/disabled
- `5`: retryable upstream error after retries exhausted (`408`, `5xx`, timeouts/connectivity)
- `6`: non-retryable Datadog API error (`4xx` except auth/rate-limit)

Retry controls:

- `--retries <N>` (default `3`)
- `--retry-backoff-ms <MS>` (default `250`)
- `--retry-max-backoff-ms <MS>` (default `5000`)
- `--retry-rate-limit=<true|false>` (default `true`)
- `--retry-timeout-seconds <N>` (default `60`, set `0` for no cap)
- `--timeout-seconds <N>` (default `30`)

### 429 Rate-Limit Retry Behavior

On HTTP 429, the tool resolves the retry sleep duration in this priority:

1. `Retry-After` response header (seconds or HTTP-date)
2. `X-RateLimit-Reset` header (seconds until limit resets)
3. Exponential backoff (from `--retry-backoff-ms`, capped at `--retry-max-backoff-ms`)

Retries are also capped by `--retry-timeout-seconds` to prevent indefinite waiting.

## Release Artifacts

Release workflow: `.github/workflows/release.yml`

Targets:

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Published files:

- platform archives (`.tar.gz` / `.zip`)
- `checksums.txt`
- `checksums.txt.sig`
- `checksums.txt.pem`
- GitHub artifact attestation

## Changelog

### v0.3.0

**Structured logging and 429 rate-limit overhaul.**

- Added JSONL structured logging to stderr via `tracing` + `tracing-subscriber`
  - `--log-level` flag (error/warn/info/debug/trace, default info)
  - `--log-file <PATH>` flag for dual-output logging (stderr + file)
  - All log output is valid JSONL, compatible with `jq` for pipeline debugging
- Rewrote 429 rate-limit retry logic:
  - Retry sleep now uses `Retry-After` header first, then `X-RateLimit-Reset` header, then exponential backoff
  - Added `--retry-timeout-seconds` flag to cap total retry time (default 60s)
  - Added `retried` count and `rate_limit_info` (remaining/limit/reset/period) to error JSON
  - Changed `retryable` from `false` to `true` in rate-limited error output
  - Structured tracing on every retry attempt (attempt number, sleep_ms, reason, status)
- Added credential-leak warning when `raw` command sends API keys to an external host
- Added `--compact` deprecation warning (use `--output json` instead)
- TOML config parse errors now logged as warnings instead of silently ignored
- Error response bodies are trimmed of leading/trailing whitespace
- Rate-limit header values logged as native JSON types (not Debug-formatted strings)
- Fixed clippy warnings (collapsible if-lets)
- Test coverage: 29 tests (up from 7)

### v0.2.0

- Added TOML configuration file support (`.ddq.toml`, `datadog.toml`, `~/.config/ddq/config.toml`)
- Added `--output` flag (`json` default, `pretty` for indented)
- Added retry controls: `--retries`, `--retry-backoff-ms`, `--retry-max-backoff-ms`, `--retry-rate-limit`

### v0.1.0

- Initial release
- `logs`, `metrics`, `events`, `raw` commands
- Exponential backoff retries for 5xx and transport errors
- JSON error envelope with deterministic exit codes
