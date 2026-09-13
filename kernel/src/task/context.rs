#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rip: u64,
}

impl Context {
    pub const fn empty() -> Self {
        Self {
            r15: 0, r14: 0, r13: 0, r12: 0, rbp: 0, rbx: 0, rip: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9:  u64,
    pub r8:  u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,

    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedThreadState {
    Cooperative(u64), // RSP points to Context
    Preemptive(u64),  // RSP points to InterruptContext
}

// Ensure the enum can be easily accessed from assembly by matching Rust layout.
// A simpler way to pass this back to assembly is an integer + pointer.
// We will return a tuple `(u64, u64)` from scheduler tick to assembly:
// (tag, rsp) where tag=0 is Cooperative, tag=1 is Preemptive.
