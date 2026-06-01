# srvcs-range

## Name

| Field | Value |
| --- | --- |
| Service | `srvcs-range` |
| Slug | `range` |
| Repository | `srvcs/range` |
| Package | `srvcs-range` |
| Kind | `orchestrator` |

## Function

comparison: range (max - min) of a list

## Dependencies

| Dependency | Repository |
| --- | --- |
| `srvcs-sortascending` | [srvcs/sortascending](https://github.com/srvcs/sortascending) |
| `srvcs-subtract` | [srvcs/subtract](https://github.com/srvcs/subtract) |

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity |
| `POST` | `/` | Evaluate the service function |
| `GET` | `/healthz` | Liveness probe |
| `GET` | `/readyz` | Readiness probe |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/openapi.json` | OpenAPI document |

## Inputs

| Name | Type | Required |
| --- | --- | --- |
| `values` | `json[]` | yes |

## Outputs

| Name | Type |
| --- | --- |
| `values` | `json[]` |
| `result` | `integer` |

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |
| `SRVCS_SORTASCENDING_URL` | `` | Base URL for srvcs-sortascending |
| `SRVCS_SUBTRACT_URL` | `http://127.0.0.1:8087` | Base URL for srvcs-subtract |

## Error Behavior

- `422` means the request could not be evaluated for the documented input shape.
- `503` means a required dependency was unavailable or returned an unexpected response.
- Dependency validation errors are forwarded when this service delegates validation.

## Local Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See the [srvcs service standard](https://github.com/srvcs/platform/blob/main/STANDARD.md) for the full operational contract.

## Metadata

Machine-readable service metadata lives in `srvcs.yaml`. Keep it aligned with this README when the service contract changes.
