#!/bin/bash
set -e

echo "=> Building userspace binaries..."
cd userspace
cargo build --release

echo "=> Packaging initramfs.tar..."
mkdir -p ../target/initramfs
cp ../target/x86_64-unknown-none/release/init ../target/initramfs/
cp ../target/x86_64-unknown-none/release/terminal_service ../target/initramfs/
cp ../target/x86_64-unknown-none/release/keyboard_service ../target/initramfs/
cp ../target/x86_64-unknown-none/release/ata_service ../target/initramfs/
cp ../target/x86_64-unknown-none/release/fat16_service ../target/initramfs/
cp ../target/x86_64-unknown-none/release/storage_test ../target/initramfs/
cp ../target/x86_64-unknown-none/release/rtl8139_service ../target/initramfs/

cp ../target/x86_64-unknown-none/release/network_service ../target/initramfs/
cp ../target/x86_64-unknown-none/release/network_test ../target/initramfs/

cd ../target/initramfs
# Use standard ustar format
tar -H ustar -cf ../initramfs.tar init terminal_service keyboard_service ata_service fat16_service storage_test rtl8139_service network_service network_test
cd ../..

echo "=> initramfs.tar created at target/initramfs.tar"
