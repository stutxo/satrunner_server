FROM rust:latest as build

WORKDIR /src/
COPY . .


RUN cargo build --release

FROM gcr.io/distroless/cc-debian12

COPY --from=build /src/target/release/satrunner_server /usr/local/bin/server

WORKDIR /usr/local/bin

ENV RUST_LOG="info"

CMD ["server"]
