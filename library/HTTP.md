# The Rust HTTP surface

`library/api` serves the calculation library over HTTP. It exists so the Python
services can be deleted: today a FastAPI service is the only thing serving its
endpoint, so retiring it before the library answers on the wire would take the
endpoint down.

## Running it

    cd library/api && cargo run --release

`VIMBAI_API_ADDR` sets the bind address, default `0.0.0.0:8099`.

## Routes

    GET  /health          engine, calculator count
    GET  /calculators     the names that answer
    POST /calc/<name>     JSON body in, JSON body out

An unknown name is a 404 that lists what does exist, so a client can recover without
a deploy.

## Money on the wire

Amounts leave the library as decimal strings, never as floats. A price of
123456789012345678901234.56 keeps every digit, and the same number through an f64
would not. A Python service in front of the seam may still publish floats for
backward compatibility; the conversion belongs at that edge and nowhere earlier.

## Adding a calculator

1. Write the maths in the domain crate, `#![forbid(unsafe_code)]`, money as `Money`.
2. Take a `&Value`, return `Result<Value, String>`.
3. Insert it in `calculators()` in `api/src/lib.rs`.
4. Add a test in `api/tests/http.rs` if it carries money or has a boundary case.
5. Bind the same function in `library/native` for the Python seam, so the two
   surfaces answer from one implementation.
