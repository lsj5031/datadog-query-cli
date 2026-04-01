# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-04-01

### Added

- JSONL structured logging to stderr via `tracing` + `tracing-subscriber`
- `--log-level` flag: error, warn, info (default), debug, trace
- `--log-file <PATH>` flag for dual-output logging (stderr + file)
- `--retry-timeout-seconds` flag to cap total retry time (default 60s, 0 = no cap)
- `retried` count in `DatadogError::RateLimited` and `AppError::RateLimited`
- `RateLimitInfo` struct (remaining/limit/reset/period) parsed from `X-RateLimit-*` headers
- Rate-limit info included in error JSON output (`rate_limit_remaining`, `rate_limit_limit`, `rate_limit_reset`, `rate_limit_period`)
- Structured tracing on every retry attempt with fields: `attempt`, `max_retries`, `sleep_ms`, `reason`, `status`
- Credential-leak warning when `raw` command sends API keys to an external host
- `base_host()` method on `DatadogClient` for host comparison
- TOML config parse errors now logged as warnings instead of silently ignored

### Changed

- 429 retry sleep now uses `Retry-After` header first, then `X-RateLimit-Reset` header, then exponential backoff (previously: exponential backoff only)
- `retryable` changed from `false` to `true` in rate-limited error JSON output
- Error response bodies trimmed of leading/trailing whitespace before inclusion in error JSON
- Rate-limit header values logged as native JSON types instead of Debug-formatted strings (`"Some(0)"` -> `0`)
- `--compact` flag now emits a deprecation warning via `tracing::warn!`

### Fixed

- 429 retries no longer waste all retry attempts with sub-second backoff when the rate limit reset is seconds away
- Error response bodies no longer contain leading/trailing newlines from Datadog API responses
- Logged `sleep_ms` now always matches actual sleep duration
- Clippy warnings for collapsible if-lets across `app_error.rs`, `config.rs`, `datadog.rs`, `main.rs`

### Tests

- Test count increased from 7 to 29
- New tests: exponential backoff calculation, Retry-After parsing (seconds + HTTP-date + past date + garbage), X-RateLimit header parsing (full + partial + empty), rate-limited error JSON output, `compute_429_sleep_ms` priority (Retry-After > X-RateLimit-Reset > backoff), error body trimming, `base_host()` extraction

## [0.2.0] - 2025-02-20

### Added

- TOML configuration file support (`.ddq.toml`, `datadog.toml`, `~/.config/ddq/config.toml`)
- `--output` flag (`json` default, `pretty` for indented)
- Retry controls: `--retries`, `--retry-backoff-ms`, `--retry-max-backoff-ms`, `--retry-rate-limit`

## [0.1.0] - 2025-02-19

### Added

- Initial release
- `logs`, `metrics`, `events`, `raw` commands
- Exponential backoff retries for 5xx and transport errors
- JSON error envelope with deterministic exit codes (1-6)
- `--compact` flag for single-line JSON output
- Time expression support: `now`, `now-15m`, `now-1h`, `now-2d`, RFC3339, unix timestamps
