# Build the calculation API from the workspace.
#
# The context is the workspace root (rust/), not this directory: every crate here is a path
# dependency, so a build from api/ alone would not resolve them.
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p vimbai_api

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && apt-get clean
WORKDIR /app
COPY --from=build /src/target/release/vimbai-api /usr/local/bin/vimbai-api
ENV VIMBAI_API_ADDR=0.0.0.0:8099
EXPOSE 8099
# the calculators answer /health whether or not a database is configured; the record routes
# are only mounted when NEO4J_* is set
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
    CMD curl -fsS http://127.0.0.1:8099/health || exit 1
ENTRYPOINT ["/usr/local/bin/vimbai-api"]
