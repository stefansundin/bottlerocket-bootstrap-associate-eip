FROM rust:1-bookworm AS builder

ARG TARGETARCH
ARG CARGO_BUILD_JOBS

ENV DEBIAN_FRONTEND=noninteractive
ENV CC=musl-gcc
ENV AR=ar
ENV RUST_BACKTRACE=full

RUN apt-get update && apt-get install -y musl-tools

WORKDIR /src
ADD . .
RUN find

RUN rustup --version

RUN case "$TARGETARCH" in \
      arm64) TARGET=aarch64-unknown-linux-musl ;; \
      amd64) TARGET=x86_64-unknown-linux-musl ;; \
      *) echo "Does not support $TARGETARCH" && exit 1 ;; \
    esac && \
    rustup target add $TARGET && \
    cargo build --profile release-build --target $TARGET && \
    mv target/$TARGET/release-build/bottlerocket-bootstrap-associate-eip target/

# Copy the binary into an empty docker image
FROM scratch

LABEL org.opencontainers.image.authors="Stefan Sundin"
LABEL org.opencontainers.image.url="https://github.com/stefansundin/bottlerocket-bootstrap-associate-eip"

COPY --from=builder /src/target/bottlerocket-bootstrap-associate-eip /bottlerocket-bootstrap-associate-eip

# Use the CA bundle from the Bottlerocket file system
ENV SSL_CERT_FILE=/.bottlerocket/rootfs/etc/pki/tls/certs/ca-bundle.crt

ENTRYPOINT [ "/bottlerocket-bootstrap-associate-eip" ]
