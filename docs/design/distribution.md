# Cross-platform distribution
Status: proposed extension of Flutter structure and existing CI.

## Sequence
First signed Android APK for controlled testing, then store distribution after policy and quality gates. Windows, Linux, macOS, iOS and web follow demonstrated demand and platform-specific build/signing readiness. One Flutter feature codebase and shared Rust engine; platform adapters only where necessary.

## Pipeline
Existing CI builds pinned toolchains, runs Rust/Go/Flutter tests, packages per target, generates checksums and software bill of materials, signs artifacts with protected keys, publishes versioned release notes and rollback instructions. APK direct distribution requires trusted HTTPS, signing continuity and explicit update mechanism. Never distribute debug builds as production.

## Integration
Extend existing workflows rather than duplicate CI; use local runtime packaging where supported and verify FFI/process lifecycle per OS. Define API compatibility between client and engine, schema upgrade and recovery. Web uses remote engine unless verified local capability exists.

## Acceptance
Clean install, upgrade from prior release, offline startup, signature verification, device integration tests, accessibility, crash reporting consent and rollback rehearsal. Store approval and external payment eligibility are separate release gates.
