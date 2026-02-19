# datadog-query-cli

Datadog API query CLI in Rust for local AI-assisted/debug workflows where MCP is unavailable.

## Features

- Logs queries (`/api/v2/logs/events/search`)
- Metrics queries (`/api/v1/query`)
- Events queries (`/api/v2/events`)
- Generic raw Datadog API calls
- Agent-friendly JSON output and JSON error envelopes
- Retry/backoff for retryable upstream failures
- Release binaries for Linux/macOS/Windows targets

## Why this tool

- No Python/Go runtime dependency for agent tool calls
- Single purpose-built binary for Datadog query automation
- Predictable JSON contract for orchestration and retries

## Requirements

- Datadog API credentials:
  - `DD_API_KEY`
  - `DD_APP_KEY` (or `DD_APPLICATION_KEY`)
- Optional:
  - `DD_SITE` (defaults to `datadoghq.com`)

## Install

### Build from source

```bash
cargo build --release --locked
install -Dm755 target/release/datadog-query-cli ~/.local/bin/datadog-query-cli
```

### Install from GitHub release (Linux/macOS)

```bash
VERSION="v0.1.0"
REPO="<owner>/datadog-query-cli"

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

### Install from GitHub release (Windows PowerShell)

```powershell
$Version = "v0.1.0"
$Repo = "<owner>/datadog-query-cli"
$Asset = "datadog-query-cli-$Version-x86_64-pc-windows-msvc.zip"

Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$Version/$Asset" -OutFile "$env:TEMP\$Asset"
Expand-Archive -Path "$env:TEMP\$Asset" -DestinationPath "$env:TEMP\ddq" -Force
New-Item -ItemType Directory -Force "$HOME\bin" | Out-Null
Copy-Item "$env:TEMP\ddq\datadog-query-cli-$Version-x86_64-pc-windows-msvc.exe" "$HOME\bin\datadog-query-cli.exe" -Force
```

### Verify signed checksums (cosign keyless)

```bash
VERSION="v0.1.0"
REPO="<owner>/datadog-query-cli"

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

## Quickstart

```bash
export DD_API_KEY=...
export DD_APP_KEY=...
export DD_SITE=datadoghq.com

datadog-query-cli \
  --output json \
  logs "service:api @http.status_code:[500 TO 599]" \
  --from now-30m --to now --limit 20
```

## Agent Contract

### Output modes

- `--output json` (default): compact JSON for machine parsing
- `--output pretty`: pretty JSON for local debugging
- `--compact`: deprecated alias for compact JSON

### Exit codes

- `0`: success
- `2`: usage/config/input error
- `3`: auth error (`401`/`403`)
- `4`: rate-limited (`429`) after retries are exhausted/disabled
- `5`: retryable upstream error after retries are exhausted (timeouts, connect failures, `5xx`, `408`)
- `6`: non-retryable Datadog API error (`4xx` except auth/rate-limit)
- `1`: internal serialization/runtime error

### Error envelope format

Errors are written to `stderr` as JSON:

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

### Retry flags

- `--retries <N>`: retry attempts (default `3`)
- `--retry-backoff-ms <MS>`: base backoff (default `250`)
- `--retry-max-backoff-ms <MS>`: max backoff cap (default `5000`)
- `--retry-rate-limit=<true|false>`: retry `429` responses (default `true`)
- `--timeout-seconds <N>`: per-request timeout (default `30`)

## Usage

Global flags:

- `--site`: Datadog site suffix or full API URL
- `--api-key`: override `DD_API_KEY`
- `--app-key`: override `DD_APP_KEY`/`DD_APPLICATION_KEY`
- `--output`: `json` or `pretty`
- `--retries`, `--retry-backoff-ms`, `--retry-max-backoff-ms`, `--retry-rate-limit`, `--timeout-seconds`

### Logs query

```bash
datadog-query-cli --output json \
  logs "env:prod service:web" \
  --from now-1h --to now --limit 50 --sort desc
```

### Metrics query

```bash
datadog-query-cli --output json \
  metrics "avg:system.cpu.user{host:my-host}" \
  --from now-15m --to now
```

### Events query

```bash
datadog-query-cli --output json \
  events --query "service:web status:error" \
  --from now-1h --to now --limit 25
```

### Raw API query

```bash
datadog-query-cli --output json raw \
  --method GET \
  --path /api/v1/validate
```

With params:

```bash
datadog-query-cli --output json raw \
  --method GET \
  --path /api/v1/monitor \
  --query page=0 \
  --query per_page=10
```

With body:

```bash
datadog-query-cli --output json raw \
  --method POST \
  --path /api/v2/logs/events/search \
  --body '{"filter":{"query":"service:api","from":"now-15m","to":"now"},"page":{"limit":10}}'
```

## Agent Tool Examples

### OpenAI Responses API tool schema

```json
{
  "type": "function",
  "name": "datadog_query",
  "description": "Run Datadog queries via datadog-query-cli and return JSON",
  "parameters": {
    "type": "object",
    "properties": {
      "subcommand": { "type": "string", "enum": ["logs", "metrics", "events", "raw"] },
      "args": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Exact CLI args after subcommand"
      }
    },
    "required": ["subcommand", "args"]
  }
}
```

Example tool execution command:

```bash
datadog-query-cli --output json --retries 3 "$subcommand" "${args[@]}"
```

### LangChain (Python) wrapper

```python
import json
import subprocess
from langchain.tools import tool

@tool
def datadog_query(args: list[str]) -> dict:
    cmd = ["datadog-query-cli", "--output", "json", "--retries", "3", *args]
    p = subprocess.run(cmd, text=True, capture_output=True)
    stream = p.stdout if p.returncode == 0 else p.stderr
    return {"exit_code": p.returncode, "payload": json.loads(stream)}
```

### AutoGen-style shell tool config

```python
def run_ddq(args: list[str]) -> dict:
    cmd = ["datadog-query-cli", "--output", "json", *args]
    p = subprocess.run(cmd, text=True, capture_output=True)
    body = p.stdout if p.returncode == 0 else p.stderr
    return {"ok": p.returncode == 0, "exit_code": p.returncode, "json": json.loads(body)}
```

## GitHub Releases

`.github/workflows/release.yml` builds and publishes artifacts for:

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Each release includes:

- `.tar.gz` / `.zip` binaries
- `checksums.txt`
- `checksums.txt.sig`
- `checksums.txt.pem`
- GitHub provenance attestation (Artifact Attestations)
