# Parity fixtures

Each file records what a retired Python service answered, captured while the service was still on disk, together with the
addresses it published and the request that produced the answer. They are evidence, not configuration: nothing reads
them at runtime, and `rust/tests/financial-integrity/tests/parity.rs` asserts that the Rust engine still answers the same
figures at the same addresses.

A fixture is added when a service is retired with figures worth keeping. A fixture is never edited to match new output:
if the engine's answer changes, the change is a decision, and it is recorded here deliberately with a note saying why.
