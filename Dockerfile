FROM rust:1.85-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY benches/ benches/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/tokenscavenger /usr/local/bin/tokenscavenger
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/tokenscavenger"]
CMD ["-c", "/etc/tokenscavenger/tokenscavenger.toml"]
