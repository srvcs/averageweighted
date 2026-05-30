# srvcs-averageweighted

The weighted-average orchestrator of the srvcs.cloud distributed standard
library.

Its single concern: **arithmetic: weighted average.** It owns the *control flow*
— composing three float primitives — but does no arithmetic of its own. It asks
[`srvcs-floatmultiply`](https://github.com/srvcs/floatmultiply) for each
`value * weight`, folds those products with
[`srvcs-floatadd`](https://github.com/srvcs/floatadd), folds the weights with
`srvcs-floatadd` too, and finally asks
[`srvcs-floatdivide`](https://github.com/srvcs/floatdivide) to divide the two
sums.

```
averageweighted(values, weights):
    require len(values) == len(weights) and not empty   # else 422
    sumwv = fold floatadd over floatmultiply(values[i], weights[i])  # start 0
    sumw  = fold floatadd over weights                              # start 0
    return floatdivide(sumwv, sumw)                                 # an f64
```

`averageweighted([1, 2, 3], [1, 2, 3]) == (1 + 4 + 9) / 6 ~= 2.3333333333333335`.

The result is a floating-point number (an f64) and may be fractional.

Validation is not handled here. This service never calls `srvcs-isnumber`
directly; instead its dependencies validate their own operands, and any `422`
they raise is forwarded verbatim. The one local guard is the shape check:
`values` and `weights` must be non-empty and of equal length.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute the weighted average |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' \
  -d '{"values": [1, 2, 3], "weights": [1, 2, 3]}'
# {"values":[1,2,3],"weights":[1,2,3],"result":2.3333333333333335}
```

Responses:

- `200 {"values": [...], "weights": [...], "result": n}` — evaluated; `result`
  is a float (an f64, possibly fractional).
- `422` — invalid input (lengths differ or empty:
  `{"error":"values and weights must be non-empty and equal length"}`), or a
  dependency rejected an operand and the `422` is forwarded verbatim.
- `500` — a reachable dependency returned a `200` without a numeric `result`
  (a contract violation).
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-floatmultiply`](https://github.com/srvcs/floatmultiply)
- [`srvcs-floatadd`](https://github.com/srvcs/floatadd)
- [`srvcs-floatdivide`](https://github.com/srvcs/floatdivide)

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_FLOATMULTIPLY_URL` | `http://127.0.0.1:8090` | Base URL of `srvcs-floatmultiply` |
| `SRVCS_FLOATADD_URL` | `http://127.0.0.1:8091` | Base URL of `srvcs-floatadd` |
| `SRVCS_FLOATDIVIDE_URL` | `http://127.0.0.1:8092` | Base URL of `srvcs-floatdivide` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up *computing* mock `srvcs-floatmultiply`,
`srvcs-floatadd` and `srvcs-floatdivide` services in-process — they read the
request body and return the real `a * b` / `a + b` / `a / b` as floats, so the
composition is genuinely exercised against the asserted cases (float results are
compared approximately). See
[`srvcs/platform`](https://github.com/srvcs/platform) for the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
