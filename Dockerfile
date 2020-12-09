FROM rust:1.48.0 as planner
WORKDIR app
RUN cargo install --version 0.1.9 cargo-chef
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.48.0 as cacher
WORKDIR app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json

FROM rust:1.48.0 as builder
WORKDIR app
COPY src ./src
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build

FROM debian:buster-slim as runtime
RUN apt update && apt install -y libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/debug/poodle /usr/local/bin/poodle
COPY poodle.toml /etc/poodle/poodle.toml
CMD ["poodle"]
