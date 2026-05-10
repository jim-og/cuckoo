# syntax=docker/dockerfile:1.7

FROM rust:1-slim AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --bin cuckoo

FROM debian:stable-slim AS runtime

RUN groupadd --system --gid 1000 cuckoo \
 && useradd  --system --uid 1000 --gid cuckoo --no-create-home --shell /usr/sbin/nologin cuckoo

COPY --from=builder /app/target/release/cuckoo /usr/local/bin/cuckoo

USER cuckoo
EXPOSE 3000
ENV PORT=3000

ENTRYPOINT ["/usr/local/bin/cuckoo"]
