use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::ipc::Endpoint;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Rights: u32 {
        const NONE    = 0;
        const SEND    = 1 << 0;
        const RECEIVE = 1 << 1;
        const WAIT    = 1 << 2;
        const ALL     = Self::SEND.bits() | Self::RECEIVE.bits() | Self::WAIT.bits();
    }
}

#[derive(Clone)]
pub enum Capability {
    Endpoint(Arc<Endpoint>, Rights),
    Process(Arc<spin::Mutex<crate::task::process::ProcessState>>, Rights),
    Interrupt(u8),
    PortIO(u16, u16, u8), // start, len, allowed_width (0 = any)
    MemoryMap(u64, u64),
}

pub struct CSpace {
    pub slots: Vec<Option<Capability>>,
}

impl CSpace {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(), // Vec::new() is a const fn
        }
    }

    pub fn insert(&mut self, cap: Capability) -> usize {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(cap);
                return i;
            }
        }
        self.slots.push(Some(cap));
        self.slots.len() - 1
    }

    pub fn insert_at(&mut self, index: usize, cap: Capability) {
        if index >= self.slots.len() {
            self.slots.resize(index + 1, None);
        }
        self.slots[index] = Some(cap);
    }

    pub fn get(&self, handle: usize) -> Option<Capability> {
        self.slots.get(handle)?.clone()
    }

    pub fn remove(&mut self, handle: usize) -> Option<Capability> {
        if let Some(slot) = self.slots.get_mut(handle) {
            slot.take()
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum CapError {
    InvalidHandle,
    PermissionDenied,
    WrongType,
}
