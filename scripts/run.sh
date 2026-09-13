#!/bin/bash
set -e

echo "=> Building initramfs..."
./scripts/build_initramfs.sh

echo "=> Building ELSE kernel..."
cd kernel
cargo build --release "$@"
cd ..

echo "=> Preparing ISO structure..."
mkdir -p iso_root
cp target/x86_64-unknown-none/release/kernel iso_root/

echo "=> Downloading Limine if needed..."
if [ ! -d "limine" ]; then
    git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1
    make -C limine
fi

echo "=> Copying Limine files..."
cp limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/
mkdir -p iso_root/EFI/BOOT
cp limine/BOOTX64.EFI iso_root/EFI/BOOT/
cp limine/BOOTIA32.EFI iso_root/EFI/BOOT/

echo "=> Creating limine.conf..."
cat > iso_root/limine.conf << EOF
timeout: 0
/ELSE OS
    protocol: limine
    kernel_path: boot():/kernel
EOF

echo "=> Creating ISO image..."
xorriso -as mkisofs -b limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    --efi-boot limine-uefi-cd.bin \
    -efi-boot-part --efi-boot-image --protective-msdos-label \
    iso_root -o else.iso

echo "=> Installing Limine to ISO..."
./limine/limine bios-install else.iso

echo "=> Creating storage disk (disk.img)..."
if [ ! -f disk.img ]; then
    dd if=/dev/zero of=disk.img bs=1M count=10
fi

if [ -z "$NO_QEMU" ]; then
    qemu-system-x86_64 -cdrom else.iso \
        -drive file=disk.img,format=raw,index=0,media=disk \
        -netdev user,id=net0,hostfwd=udp::8080-:8080 \
        -device rtl8139,netdev=net0 \
        -serial file:serial.log -no-reboot
else
    echo "=> Skipping QEMU execution (NO_QEMU is set)."
fi
