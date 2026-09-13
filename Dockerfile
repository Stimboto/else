# Development environment for ELSE OS
FROM ubuntu:24.04

# Install required packages
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    qemu-system-x86 \
    nasm \
    xorriso \
    grub-pc-bin \
    grub-common \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain (1.98.0 as requested)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.98.0
ENV PATH="/root/.cargo/bin:${PATH}"

# Install required components for no_std kernel development
RUN rustup component add rust-src

WORKDIR /workspace
CMD ["bash"]
