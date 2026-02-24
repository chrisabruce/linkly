# ── Stage 1: build ─────────────────────────────────────────────────────────
FROM rust:slim AS builder

WORKDIR /app

# Copy the server workspace into the build container
COPY server/ .

RUN cargo build --release --bin linkly

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM debian:bookworm-slim

# ca-certificates is needed by reqwest/rustls to verify TLS when doing
# IP geolocation lookups
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/linkly /app/linkly

# The data directory is where the SQLite database lives.
# Mount a persistent volume here to survive container restarts.
RUN mkdir -p /data

EXPOSE 8080

CMD ["/app/linkly"]
