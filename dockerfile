FROM rust:latest as build

WORKDIR /src/
COPY . .

RUN cargo build --release

FROM gcr.io/distroless/cc-debian10

COPY --from=build /src/target/release/satrunner_server /usr/local/bin/server

WORKDIR /usr/local/bin

CMD ["server"]
