# ELSE Research Notebook

## Motivation
Modern operating systems are typically written in C/C++, leading to memory safety vulnerabilities. While microkernels provide strong isolation, they often suffer from IPC overhead and complex state management. Rust offers memory safety without a garbage collector, which could enable safer kernel design without sacrificing performance.

## Research Problem
Balancing fault isolation, modularity, and performance in a capability-oriented kernel is challenging. Existing microkernels enforce isolation via address spaces, while language-based systems (like Singularity or Theseus) rely on the language runtime/type system.

## Tentative Research Question
*Can a lightweight capability-oriented service architecture implemented predominantly in Rust provide practical fault isolation and recovery while maintaining acceptable performance and implementation complexity for a small x86_64 operating system?*

## Initial Hypothesis
(Tentative) By combining Rust's compile-time memory safety with capability-oriented access control and hardware-enforced virtual address space isolation, it is possible to build an operating system where user-space services can fault and recover transparently, with an implementation complexity significantly lower than traditional formally-verified microkernels.

## Related Systems
- **Theseus**: Language-assisted OS modularity and Rust-based systems design.
- **RedLeaf**: Isolation and fault recovery.
- **seL4**: Capability-oriented security and isolation.
- **Barrelfish**: Multicore OS architecture.
- **RustBelt**: Foundations of Rust safety.
- **Redox**: Practical Rust operating-system engineering.

## Possible Research Gap
(Tentative) Bridging the gap between pure language-level isolation and pure hardware-level capability isolation using pragmatic Rust design for x86_64, specifically focusing on the recoverability of core OS services without requiring a full microkernel redesign or formal verification.

## Possible Contributions
- A pragmatic capability-based IPC model in Rust.
- A user-space service recovery mechanism tailored to Rust's panic semantics.
- Evaluation of the overhead of capability checks vs. language-level safety.
- An assessment of bitmap allocator efficiency and metadata overhead in a capability-oriented Rust kernel.

## Evaluation Ideas
- Measure IPC round-trip time between isolated services.
- Measure the time to detect a fault, tear down, and restart a service.
- Compare system call overhead to Linux/seL4.
- Code complexity (lines of unsafe code, component size).
- Measure physical frame allocator overhead (average cycles per allocation/deallocation).
- Evaluate fragmentation and bitmap memory footprint versus system uptime.

## Open Questions
- How to efficiently map hardware interrupts to user-space services?
- How to handle capability revocation safely?
- Should the IPC be purely synchronous or asynchronous?
