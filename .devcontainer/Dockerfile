# Use the official Rust image as a base
FROM rust:slim

# Install tools
RUN apt-get update && apt-get install -y \
    curl \
    git \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install just

# Set working directory
WORKDIR /workspace