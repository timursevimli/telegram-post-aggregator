FROM rust:1.81-bookworm as builder
WORKDIR /usr/src/app
RUN apt-get update && \
  apt-get install -y pkg-config libssl-dev && \
  rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /usr/local/bin
RUN apt-get update && \
  apt-get install -y ca-certificates libssl3 && \
  rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/telegram_post_aggregator .
COPY config.json .
COPY first.session .
COPY .env .

CMD ["./telegram_post_aggregator"]
