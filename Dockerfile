FROM rust:bullseye as builder

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install libclang-dev protobuf-compiler -y

WORKDIR /src
COPY . .
RUN cargo build --release


FROM ubuntu:22.04

RUN mkdir /data
VOLUME /data

WORKDIR /app
COPY --from=builder /src/target/release/distance-db-populator /src/target/release/distance-db-populator-manager ./
COPY libsteam_api.so docker-extra/* ./

RUN DEBIAN_FRONTEND=noninteractive dpkg --add-architecture i386 && \
    apt-get update -y && \
    apt-get install -y --no-install-recommends xserver-xorg-video-dummy steam \
    # Fixes Steam login error
    ca-certificates \
    # Install xdpyinfo, which we use to wait for X11 to be ready
    x11-utils && \
    #
    # Cleanup
    rm -rf /var/lib/apt/lists/* && \
    #
    chmod +x ./run.sh ./wait-x11.sh

# Add Steam to path
ENV PATH="/usr/games:${PATH}"

ENV DISPLAY=:0
CMD ./run.sh