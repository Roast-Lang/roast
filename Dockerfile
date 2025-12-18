# Roast Language - Multi-stage Build
FROM rust:1.75-slim AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    llvm-dev \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libclang1 \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/roastc /usr/local/bin/
COPY --from=builder /app/target/release/kitchen /usr/local/bin/

# Copy runtime library
COPY --from=builder /app/target/release/libroast_runtime.so /usr/local/lib/

# Set library path
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV ROAST_HOME=/usr/local/share/roast

# Create workspace directory
WORKDIR /workspace

ENTRYPOINT ["roastc"]
CMD ["--help"]
