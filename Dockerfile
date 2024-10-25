# Use Rust as the builder based on Alpine
FROM rust:1.82.0-alpine3.20 AS builder
WORKDIR /src
COPY . /src/
RUN set -xe \
  && apk add --no-cache musl-dev libressl-dev \
  && cargo build --release

# Use Alpine as the runner
FROM alpine:3.20 AS runner
COPY --from=builder /src/target/release/miragend /usr/local/bin/miragend
ENV RUST_LOG=info
EXPOSE 8080
ENTRYPOINT [ "miragend" ]
