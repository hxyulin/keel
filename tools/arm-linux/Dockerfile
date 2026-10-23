FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential clang lld llvm libclang-rt-18-dev binutils curl ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
# Keep Linux build artifacts separate from a mounted macOS target directory.
ENV CARGO_TARGET_DIR=/tmp/keel-target
WORKDIR /workspace
CMD ["bash"]
