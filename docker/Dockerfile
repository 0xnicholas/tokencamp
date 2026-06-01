FROM rust:1.85-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p gateway -p tokencamp

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/gateway /usr/local/bin/tokencamp-gateway
COPY --from=builder /app/target/release/tokencamp /usr/local/bin/tokencamp
COPY config/ /app/config/
ENV OPENAI_API_KEY=""
EXPOSE 3000
CMD ["tokencamp-gateway"]
