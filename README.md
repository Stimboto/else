<p align="center">
  <img src="https://github.com/user-attachments/assets/34f216a3-3bae-494e-bd28-25e505dee7a6" alt="ELSE OS Banner" width="100%">
</p>

<div align="center">
  <h1>ELSE</h1>
  <p><strong>A from-scratch x86_64 research operating system written in Rust.</strong></p>

  ![RUST](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)
  ![ARCHITECTURE](https://img.shields.io/badge/Architecture-x86__64-blue?style=flat-square)
  ![PLATFORM](https://img.shields.io/badge/Platform-Bare%20Metal-orange?style=flat-square)
  ![BOOTLOADER](https://img.shields.io/badge/Bootloader-Limine-3DBB00?style=flat-square)
  ![STATUS](https://img.shields.io/badge/Status-Active-success?style=flat-square)
</div>

---

## Architectural Philosophy

- **Rust no_std kernel**: Leverages Rust's memory safety, type system, and ownership model for the core kernel.
- **Higher-half kernel**: The kernel runs in the higher half of the virtual address space, mapped into all user processes.
- **Hardware-enforced address spaces**: Strict separation between processes using x86_64 paging.
- **Capability-oriented security**: Explicit capability-mediated resource access (Endpoints, Memory, PortIO, IRQs, etc.).
- **IPC**: Message-passing IPC for all cross-process communication.
- **User-space services**: Higher-level functionality (Filesystems, Networking) is moved to user space.
- **User-space drivers**: Hardware drivers (like RTL8139, ATA) operate in Ring 3 with bounded capabilities.
- **Fault isolation**: Service-level fault containment via process isolation.
- **QEMU-based reproducibility**: Containerized development and reproducible emulation workflow.
- **Research instrumentation**: Built to enable future architectural research and benchmarking.

## Current Status

**STAGE 1 — OS CORE / RESEARCH PLATFORM**: Phases 0–12 COMPLETE  
**STAGE 2 — DESKTOP / USER EXPERIENCE**: NOT STARTED YET

> [!NOTE]
> ELSE is a research operating-system project with a core microkernel-style user-space service architecture. It is NOT intended to replace Linux or Windows, and does not make claims of production readiness or formal verification. It currently lacks a GUI, SMP, TCP/IPv6, and other general-purpose desktop features.

## Verified Features (Stage 1)

**Networking (Phase 11):**  
Verified Ethernet + ARP + IPv4 + UDP stack.  
Host → ELSE → Host UDP communication is verified via QEMU SLIRP.  
Current networking path: `PCI → RTL8139 → DMA → Ethernet → ARP → IPv4 → UDP → network_service → network_test → QEMU SLIRP → Host`

## Build and Development

### 1. Docker (Reproducible Build Workflow)

To guarantee a reproducible build environment with all necessary dependencies:

```bash
./scripts/docker-build.sh
```
This builds the Docker image and compiles the `else.iso` image inside the container.

### 2. Native / WSL Workflow
Ensure you have Rust 1.98.0, `qemu-system-x86_64`, `nasm`, `xorriso`, and `build-essential` installed.
```bash
bash scripts/run.sh
```
This builds the initramfs, compiles the kernel, generates `else.iso`, and launches QEMU.

### 3. QEMU Execution
If you built via Docker, run QEMU natively on your host/WSL to ensure the GUI displays properly:
```bash
bash scripts/run.sh
```
*(If QEMU is run within Docker, it may not display a window unless X11 forwarding is explicitly configured).*

### 4. Networking Test (Phase 11)
After launching QEMU, the `network_test` application listens on UDP port 8080.
From your host (or WSL), send a UDP packet:
```bash
echo 'ELSE_PING' | nc -u -w 2 127.0.0.1 8080
```
You should see the payload echo back to your host terminal.
