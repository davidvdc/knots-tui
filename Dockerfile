FROM --platform=linux/amd64 rust:latest AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src/ src/
RUN cargo build --release

FROM scratch AS export
COPY --from=builder /app/target/release/knots-tui /knots-tui
