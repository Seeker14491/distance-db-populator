FROM rust:bookworm as builder

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install protobuf-compiler -y

WORKDIR /src
COPY . .
RUN cargo build --release


FROM debian:bookworm

RUN mkdir /data
VOLUME /data

WORKDIR /app
COPY --from=builder /src/target/release/distance-db-populator /src/target/release/distance-db-populator-manager ./

CMD ./distance-db-populator-manager