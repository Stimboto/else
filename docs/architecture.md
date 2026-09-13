# ELSE Architecture

## System Goals
ELSE aims to be a memory-safe, modular, capability-oriented operating system prototype for the x86_64 architecture. The system is designed to explore practical fault isolation and recovery while maintaining acceptable performance and complexity.

## Architectural Philosophy
1. Keep the privileged kernel core as small and well-defined as practical.
2. Prefer Rust's type system and ownership model over unnecessary unsafe code.
3. Isolate unavoidable unsafe hardware operations behind explicit interfaces.
4. Use hardware-enforced virtual-address-space isolation.
5. Use capability-oriented access to privileged resources.
6. Prefer explicit IPC between isolated components.
7. Move higher-level functionality toward modular services where practical.
8. Design for fault containment and service recovery.
9. Prefer simple, understandable implementations over unnecessary complexity.
10. Every subsystem must have clear interfaces and ownership boundaries.
11. Every implementation phase must preserve existing functionality.
12. Every meaningful subsystem should eventually have tests and measurable behavior.

## High-Level Architecture
```text
USER APPLICATIONS
|
USER SERVICES
|
| IPC
|
+---------------------------+
|          KERNEL           |
|                           |
| Scheduler                 |
| Virtual Memory            |
| Physical Memory           |
| IPC primitives            |
| Capability subsystem      |
| Interrupt handling        |
| Hardware abstraction      |
+---------------------------+
|
HARDWARE
```

## Component Responsibilities

### Kernel Responsibilities (Phase 3)
The kernel operates in Ring 0 and is responsible for:
- Hardware abstraction and initialization (GDT, IDT).
- Interrupt and exception handling (Double Faults, Page Faults, etc).
- Hardware timer management.
- Physical and virtual memory management (Bitmap allocator, Page tables, HHDM, Kernel Heap).
- Thread scheduling and preemption (Planned).
- IPC primitives for inter-process communication (Planned).
- Capability subsystem for capability-mediated access control (Planned).

### Architecture Abstraction (Phase 2)
The kernel abstracts x86_64 specifics into the `arch` module:
- **GDT & TSS**: Sets up kernel code/data segments and an Interrupt Stack Table (IST) specifically for double fault handlers.
- **IDT**: Configures exception handlers for Breakpoint, Double Fault, General Protection Fault, Invalid Opcode, Page Fault, and Divide-by-Zero. Fatal exceptions halt the CPU.
- **Interrupts (PIC)**: Legacy 8259 PIC is initialized to map hardware IRQs (e.g., Timer) to vectors 32-47 to avoid collision with CPU exceptions. (Future phases will migrate to APIC).
- **Timer (PIT)**: Acknowledges timer interrupts and increments an atomic tick counter, laying the groundwork for a future scheduler.
- **CPU**: Provides safe abstractions for CPU halt, interrupt enable/disable, and executing closures without interrupts.

### User-Space Responsibilities (Planned)
User-space components operate in Ring 3 and include:
- **Init/Supervisor**: System initialization and service lifecycle management.
- **System Services**: Filesystem, networking, logging, and monitoring.
- **Drivers**: Device drivers running as isolated user-space services.
- **Applications**: End-user applications (e.g., shell).

### Capability Model Concept (Planned)
Resource access is mediated by capabilities. A process must hold a valid capability to interact with hardware, memory, or other processes. The kernel manages the capability space and validates operations.

### IPC Concept (Planned)
Isolated components communicate via kernel-mediated Inter-Process Communication (IPC). The IPC mechanism should be fast, synchronous/asynchronous where appropriate, and capability-aware.

### Service Model Concept (Planned)
Higher-level functionality is implemented as isolated user-space services. Services can be composed, updated, or replaced dynamically without modifying the kernel.

### Fault-Isolation Concept (Planned)
If a service encounters a fault (e.g., panic or exception), the supervisor can terminate and restart the service without bringing down the entire system. Hardware-enforced virtual address spaces contain the fault.

### Memory Management (Phase 3)
- **Physical Memory**: Managed by a Bitmap Allocator. The allocator dynamically locates a suitable block of RAM to store its metadata, marking regions reported by the Limine bootloader memory map as either usable or reserved.
- **Virtual Memory**:
  - The Higher-Half Direct Map (HHDM) maps the entire physical address space into the kernel's virtual address space (e.g., `0xffff800000000000`), enabling `O(1)` translation between physical frames and virtual addresses.
  - Page Tables (Level-4 x86_64 structure) are managed through safe abstractions, allowing precise mapping and unmapping of 4KB pages.
  - A dynamic Kernel Heap is established at a canonical address (e.g., `0xffff900000000000`) and managed using a linked-list allocator to support dynamic structures before user-space is initialized.
- **Isolation**: Will be achieved by unique address spaces for each process (Planned).

### Scheduler Direction (Planned)
Preemptive multitasking scheduler supporting multiple threads per process.

### Filesystem Direction (Planned)
A modular filesystem service running in user-space. Specific implementation to be finalized.

### Networking Direction (Planned)
A modular networking stack running in user-space. Specific implementation to be finalized.
