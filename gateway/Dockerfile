# Build context is the repository root (see docker-compose.yml).
FROM rust:1-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY engine engine
COPY gateway gateway
RUN cargo build --release -p exchangekit-gateway

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/exchangekit-gateway /usr/local/bin/exchangekit-gateway
EXPOSE 8080
CMD ["exchangekit-gateway"]
