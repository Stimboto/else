# Technology Decisions

This document records the selected technologies for the ELSE operating system.

## Currently Selected Technologies
- **Implementation Language**: Rust (Primary)
- **Target Architecture**: x86_64
- **Execution Environment**: QEMU / KVM
- **Host Development Environment**: Linux / WSL2
- **Build System**: Cargo
- **Version Control**: Git
- **Containerization**: Docker
- **Hosting / CI**: GitHub

## Pending Decisions
The following low-level choices are deferred and to be finalized during Phase 1 or later design phases. Do not make arbitrary decisions here yet.
- **Bootloader**: Limine v8. (Selected for robust higher-half loading and UEFI/BIOS support).
- **Boot Protocol Interface**: `limine = "0.5.0"` Rust crate. (Version 0.5.0 is specifically selected because it tracks the Limine v8 API without requiring the unstable `#![feature(ptr_metadata)]` flag, ensuring full compatibility with stable Rust `1.98.0` on the `x86_64-unknown-none` target).
- **Linker Configuration**: Custom Limine `linker.ld` loading at `0xffffffff80000000` with `.requests` section preservation.
- **Rust Toolchain/Target**: `x86_64-unknown-none` using standard `core`. No custom JSON target needed.
- **Kernel Binary Format**: ELF64.
- **Interrupt Controller**: Legacy 8259 PIC via `pic8259` crate. (Phase-2 pragmatic choice for QEMU timer stability. Will migrate to Local APIC/IOAPIC in later SMP phases).
- **Timer Mechanism**: PIT (Programmable Interval Timer) mapped to IRQ0, utilizing an atomic tick counter.
- **Synchronization**: `spin` crate for `Mutex` and `Lazy` initialization, as the standard library is unavailable in `no_std`.
- **Physical Memory Allocator**: **Bitmap Allocator**. *Design Hypothesis:* A bitmap is expected to provide `O(1)` deallocation and amortized fast allocation with low metadata overhead. It natively supports contiguous frame allocations. While free-lists provide `O(1)` operations, they lack contiguous allocation support and scatter state across RAM. Buddy allocators eliminate external fragmentation but impose `O(log N)` complexity and higher metadata footprint. We hypothesize that a bitmap's lightweight overhead and contiguous support make it an excellent fit for capability-oriented microkernels. This will be evaluated experimentally.
- **Kernel Heap Allocator**: `linked_list_allocator` crate. A lightweight, simple `no_std`-compatible global allocator. Selected to prioritize establishing basic capabilities over inventing custom heap logic during Phase 3.
- **Virtual Memory Strategy**: Limine Higher-Half Direct Map (HHDM) abstraction via x86_64 `OffsetPageTable`.
- **Filesystem Implementation**: *To be finalized during Phase 7*.
- **Networking Implementation**: *To be finalized during Phase 8*.
