# srvcs-range

The range orchestrator of the srvcs.cloud distributed standard library.

Its single concern: **the range (max - min) of a list of integers.** It does no
arithmetic of its own. It composes two other services:

```text
sorted = sortascending(values).result      # one HTTP call to srvcs-sortascending
result = subtract(sorted[last], sorted[0])  # one HTTP call to srvcs-subtract
```

The **empty list** is rejected with `422`. A singleton has range `0`
(`subtract(x, x) == 0`).

```text
range([1, 5, 3]) == 4
range([7])       == 0
```

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Range (max - min) of the integers in `values` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' -d '{"values": [1, 5, 3]}'
# {"values":[1,5,3],"result":4}
```

Responses:

- `200 {"values": [...], "result": n}` — evaluated.
- `422` — empty list, or an element is not a valid integer (forwarded from a dependency).
- `500` — a dependency returned an unusable response.
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-sortascending`](https://github.com/srvcs/sortascending)
- [`srvcs-subtract`](https://github.com/srvcs/subtract)

This is an orchestrator: it never calls `srvcs-isnumber` directly. Validation
propagates from its dependencies — a non-integer element is rejected by
`srvcs-sortascending` and the resulting `422` is forwarded unchanged.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_SORTASCENDING_URL` | `http://127.0.0.1:8086` | Base URL of `srvcs-sortascending` |
| `SRVCS_SUBTRACT_URL` | `http://127.0.0.1:8087` | Base URL of `srvcs-subtract` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up mock dependencies in-process that **actually
compute** (sortascending really sorts; subtract really subtracts), so the
composition is genuinely exercised (e.g. `range([1,5,3]) == 4`). See
[`srvcs/platform`](https://github.com/srvcs/platform) for the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
