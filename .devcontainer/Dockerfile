# Use the official Rust image as a base
FROM rust:slim

# Install tools
RUN apt-get update && apt-get install -y \
    curl \
    git \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install rustup components
RUN rustup component add rustfmt clippy

RUN cargo install just

# Set working directory
WORKDIR /workspace