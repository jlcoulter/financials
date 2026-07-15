FROM rust:1.83-alpine AS builder
RUN apk add --no-cache musl-dev sqlite-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY . .
RUN touch src/main.rs && cargo build --release

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/rust-web /usr/local/bin/rust-web
WORKDIR /app
COPY src/static /app/static
ENV STATIC_DIR=/app/static
EXPOSE 3000
CMD ["rust-web"]