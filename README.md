# datadog-query-cli

Rust CLI for querying Datadog APIs from terminals and automation workflows.

## Quickstart

```bash
export DD_API_KEY=...
export DD_APP_KEY=...
export DD_SITE=datadoghq.com

datadog-query-cli --output json \
  logs "service:api @http.status_code:[500 TO 599]" \
  --from now-30m --to now --limit 20
```

## Install

### Prebuilt binary (Linux/macOS)

```bash
VERSION="v0.1.0"
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
$Version = "v0.1.0"
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
VERSION="v0.1.0"
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
- `--output`: `json` (default) or `pretty`
- `--retries`, `--retry-backoff-ms`, `--retry-max-backoff-ms`, `--retry-rate-limit`, `--timeout-seconds`
- `--compact`: deprecated alias for compact JSON output

Examples:

```bash
# Logs
datadog-query-cli --output json \
  logs "env:prod service:web" \
  --from now-1h --to now --limit 50 --sort desc

# Metrics
datadog-query-cli --output json \
  metrics "avg:system.cpu.user{host:my-host}" \
  --from now-15m --to now

# Events
datadog-query-cli --output json \
  events --query "service:web status:error" \
  --from now-1h --to now --limit 25

# Raw GET
datadog-query-cli --output json raw \
  --method GET \
  --path /api/v1/validate

# Raw POST with body
datadog-query-cli --output json raw \
  --method POST \
  --path /api/v2/logs/events/search \
  --body '{"filter":{"query":"service:api","from":"now-15m","to":"now"},"page":{"limit":10}}'
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
    "retryable": false,
    "retry_after_ms": 1000,
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
- `--timeout-seconds <N>` (default `30`)

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
