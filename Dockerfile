FROM rust:1.94 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/target/release/zwilling-tracker /usr/local/bin/zwilling-tracker
EXPOSE 3000
CMD ["zwilling-tracker"]
