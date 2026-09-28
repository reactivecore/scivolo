FROM debian:bookworm-slim AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN printf 'path-exclude /usr/share/doc/*\npath-exclude /usr/share/man/*\npath-exclude /usr/share/info/*\npath-exclude /usr/share/locale/*\n' \
      > /etc/dpkg/dpkg.cfg.d/01-nodoc

# clang and zlib build crates/snrt; bc runs `just check`; python3 runs the scripts.
RUN apt-get update -yqq && apt-get install -yqq --no-install-recommends \
        build-essential clang zlib1g-dev curl ca-certificates git bc python3 \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

ARG JUST_VERSION=1.58.0
ARG JUST_SHA256=4a5cc2f53e6f0f8c59092a6cc38291eb729d46a7dd95d3ae582008881b84931d
RUN curl --proto '=https' --tlsv1.2 -sSfL \
        "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
        -o /tmp/just.tar.gz && \
    echo "${JUST_SHA256}  /tmp/just.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/just.tar.gz -C /usr/local/bin just && \
    rm /tmp/just.tar.gz && \
    just --version

ARG RUST_VERSION=1.95.0
ENV PATH="/root/.cargo/bin:$PATH"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} && \
    rustup component add rustfmt clippy && \
    rm -rf /root/.cargo/registry /root/.cargo/git

# The builder plus what compiles Scala Native's unit tests (`just ci-suite`).
FROM builder AS suite

RUN apt-get update -yqq && apt-get install -yqq --no-install-recommends \
        openjdk-17-jdk-headless cpio \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

ARG SCALA_CLI_VERSION=1.14.0
ARG SCALA_CLI_SHA256=7a55547894ecbc9cbf8682c5b2eb3680df0a305dc490e80a491ab873601baaa7
RUN curl --proto '=https' --tlsv1.2 -sSfL \
        "https://github.com/VirtusLab/scala-cli/releases/download/v${SCALA_CLI_VERSION}/scala-cli-x86_64-pc-linux.gz" \
        -o /tmp/scala-cli.gz && \
    echo "${SCALA_CLI_SHA256}  /tmp/scala-cli.gz" | sha256sum -c - && \
    gunzip -c /tmp/scala-cli.gz > /usr/local/bin/scala-cli && \
    chmod +x /usr/local/bin/scala-cli && \
    rm /tmp/scala-cli.gz
