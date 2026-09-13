use alloc::collections::VecDeque;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Message {
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
}

use crate::task::scheduler::Thread;

#[derive(Debug, Clone, Copy)]
pub struct MessagePtr(*mut Message);
unsafe impl Send for MessagePtr {}
unsafe impl Sync for MessagePtr {}

impl MessagePtr {
    pub fn new(ptr: *mut Message) -> Self {
        Self(ptr)
    }
    pub fn as_mut(&self) -> *mut Message {
        self.0
    }
}

pub enum EndpointState {
    Idle,
    SendersWaiting(VecDeque<(alloc::boxed::Box<Thread>, Message)>),
    ReceiversWaiting(VecDeque<(alloc::boxed::Box<Thread>, MessagePtr)>),
}

pub struct Endpoint {
    pub state: Mutex<EndpointState>,
}

impl Endpoint {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(EndpointState::Idle),
        }
    }
}
