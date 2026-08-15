# syntax=docker/dockerfile:1
# Shared by Rust applications. PACKAGE is the Cargo package and BINARY is the
# executable copied into the runtime image.
FROM rust:1.97.1-bookworm AS builder
WORKDIR /src
ARG PACKAGE
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY servers ./servers
RUN test -n "${PACKAGE}" && cargo build --locked --release --package "${PACKAGE}"

FROM gcr.io/distroless/cc-debian12:nonroot
ARG BINARY
COPY --from=builder --chown=65532:65532 /src/target/release/${BINARY} /usr/local/bin/app
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/app"]

