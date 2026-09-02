# syntax=docker/dockerfile:1

FROM rust:1.98.0-slim-trixie AS builder

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release \
    && cp target/release/yomiage /yomiage

FROM gcr.io/distroless/cc-debian13:nonroot

COPY --from=builder --chown=65532:65532 /yomiage /usr/local/bin/yomiage

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/yomiage"]
