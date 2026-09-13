#!/bin/bash
set -e

# Build the Docker image
echo "=> Building Docker development environment..."
docker build -t else-os-dev .

# Run the build process inside the Docker container
echo "=> Building ELSE inside Docker..."
docker run --rm -v "$(pwd)":/workspace -e NO_QEMU=1 else-os-dev bash -c "./scripts/run.sh"

echo "=> Docker build complete!"
echo "Note: The Docker container builds the kernel, userspace, and else.iso."
echo "      To run QEMU, execute the run.sh script natively on your host/WSL,"
echo "      as QEMU GUI may not display properly from within the Docker container."
