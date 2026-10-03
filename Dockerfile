FROM rust:1.94 AS build
WORKDIR /app
# Build dependencies against a stub main so this layer stays cached until Cargo.toml/Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY src ./src
# touch: the real main.rs can be older than the stub's build output, which would make cargo skip it.
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/target/release/zwilling-tracker /usr/local/bin/zwilling-tracker
EXPOSE 3000
CMD ["zwilling-tracker"]
