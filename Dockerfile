FROM rust:1-bullseye AS builder

ARG TARGETARCH

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y musl-tools

WORKDIR /src

ENV CC=musl-gcc
ENV AR=ar
ENV RUST_BACKTRACE=full

ADD . .

RUN rustup --version

RUN case "$TARGETARCH" in \
      arm64) TARGET=aarch64-unknown-linux-musl ;; \
      amd64) TARGET=x86_64-unknown-linux-musl ;; \
      *) echo "Does not support $TARGETARCH" && exit 1 ;; \
    esac && \
    rustup target add $TARGET && \
    cargo build --release --target $TARGET && \
    mv target/$TARGET/release/bottlerocket-bootstrap-associate-eip target/release/

# Reduce the size of the binary
RUN ls -l target/release/bottlerocket-bootstrap-associate-eip
RUN strip -s target/release/bottlerocket-bootstrap-associate-eip
RUN ls -l target/release/bottlerocket-bootstrap-associate-eip


# Copy the binary into an empty docker image
FROM scratch

LABEL org.opencontainers.image.authors="Stefan Sundin"
LABEL org.opencontainers.image.url="https://github.com/stefansundin/bottlerocket-bootstrap-associate-eip"

COPY --from=builder /src/target/release/bottlerocket-bootstrap-associate-eip /bottlerocket-bootstrap-associate-eip

# Use the CA bundle from the Bottlerocket file system
ENV SSL_CERT_FILE=/.bottlerocket/rootfs/etc/pki/tls/certs/ca-bundle.crt

ENTRYPOINT [ "/bottlerocket-bootstrap-associate-eip" ]
