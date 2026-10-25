# Telemetry

Slate can send OpenTelemetry traces to Axiom. Telemetry is off by default.
No telemetry code runs unless `AXIOM_TOKEN` is set.

## Axiom setup

1. Create an Axiom dataset for traces. The default name is `slate-traces`.
2. Create an API token with ingest permission for that dataset.
3. Set the environment before you launch Slate:

```sh
export AXIOM_TOKEN="xaat-..."
export AXIOM_DATASET="slate-traces"
export AXIOM_OTLP_ENDPOINT="https://api.axiom.co"
export OTEL_SERVICE_NAME="slate-desktop"
export RUST_LOG="info"
```

`AXIOM_OTLP_ENDPOINT` can be either `https://api.axiom.co` or the full
`https://api.axiom.co/v1/traces` URL. The application adds `/v1/traces` once.
`OTEL_EXPORTER_OTLP_ENDPOINT` is used as a fallback when
`AXIOM_OTLP_ENDPOINT` is not set.

The exporter sends these headers:

```text
Authorization: Bearer <AXIOM_TOKEN>
X-Axiom-Dataset: <AXIOM_DATASET>
```

## What is sent

The trace stream contains operation names, durations, photo counts, page
counts, formats, and error status. It does not contain file paths, image data,
project names, OAuth tokens, or API tokens.

Instrumented operations:

- Project load and save.
- Import metadata probe and preview decode, at trace level.
- Sheet planning and PDF export.
- Batch conversion.
- Per-page PDF work, at debug level.

The default trace filter is `info`. Set `RUST_LOG` to `debug` or `trace` only
when you need the higher-volume spans.

## Flush and failure behaviour

The batch span processor sends spans in the background. Slate flushes it when
the process exits. If the exporter cannot start, Slate continues without
telemetry.

## Local logs

With no `AXIOM_TOKEN`, set `RUST_LOG` to write plain-text tracing output to
stderr. This is useful for local diagnosis without a collector.
