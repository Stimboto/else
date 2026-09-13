# ELSE Development Rules

To ensure a stable, understandable, and research-focused development process, all contributors (human and AI) must adhere to the following rules:

## 1. Incremental Implementation
- Proceed through the roadmap phases sequentially.
- Do not jump ahead to implement user-space before the kernel primitives are stable.

## 2. No Large Rewrites Without Justification
- If a subsystem works, do not rewrite it merely for aesthetics or to try a new pattern.
- Any large rewrite must be justified in an architecture document update.

## 3. Preserve Subsystem Boundaries
- Every subsystem (scheduler, memory manager, IPC) must have clear interfaces and ownership boundaries.
- Do not let implementation details leak across boundaries.

## 4. Test Before Proceeding
- Do not move to the next development phase until the current phase is tested.
- Do not claim a feature is complete without verification.

## 5. Document Architectural Decisions
- Whenever a major design choice is made, document it in `architecture.md` or a dedicated RFC file.

## 6. Minimize Unsafe Rust
- Prefer Rust's type system and ownership model.
- Unsafe code is necessary for hardware interaction but should be contained and abstracted quickly.

## 7. Explain Every Significant Unsafe Block
- Every non-trivial `unsafe` block must have a `// SAFETY: ...` comment explaining why it is safe.

## 8. Version Control Practices
- Prefer small, reviewable commits.
- Use Git commits after reaching stable milestones to prevent loss of working state.

## 9. Build and Run Workflow
- **Prerequisites**: Rust toolchain (`rustup`), `nasm`, `xorriso`, `qemu-system-x86_64`, `make`, `gcc`.
- **Build & Run**: Execute `bash scripts/run.sh` from the root directory.
- **Expected Output**: QEMU will launch, and serial output will be printed to the terminal showing `[ELSE] booting`, `[BOOT] Interrupts enabled.`, and `[TIMER]` heartbeats.
- **Troubleshooting**: If Limine fails to build, ensure `build-essential` and `mtools` are installed.

## 10. Exception Testing
- Exception triggers (e.g., deliberate breakpoints) are not compiled into the normal boot path.
- To test exceptions, run the build with the specific feature flag: `cargo build --release --features test_exceptions`.
- When booted with this feature, the kernel will trigger an exception after initialization, log the `[EXCEPTION]` details, and halt.

## 11. Memory Testing
- Memory management tests (e.g. dynamic allocation checks, page mapping verifications) are isolated behind the `test_memory` feature flag.
- To run memory validation checks during boot, use: `cargo build --release --features test_memory`.
- This ensures the standard boot path remains pristine while enabling robust development diagnostics.
