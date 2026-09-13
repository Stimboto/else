use alloc::collections::VecDeque;
use alloc::boxed::Box;
use spin::Mutex;
use core::sync::atomic::{AtomicU64, Ordering};
use alloc::vec;

use super::context::{Context, InterruptContext, SavedThreadState};

// FFI to our assembly functions
extern "C" {
    pub fn switch_context(new_stack: u64) -> u64;
    pub fn switch_context_to_preemptive(new_stack: u64);
}

use alloc::sync::Arc;
use crate::capability::CSpace;

#[derive(Clone)]
pub struct Thread {
    pub id: u64,
    pub state: SavedThreadState,
    pub stack: Arc<Box<[u8]>>, // Changed to Arc to allow Cloning if needed, or we can just not Clone Thread
    pub cr3: Option<u64>,
    pub cspace: Option<Arc<Mutex<CSpace>>>,
    pub process_state: Option<Arc<Mutex<crate::task::process::ProcessState>>>,
}

impl Thread {
    const STACK_SIZE: usize = 1024 * 8; // 8 KiB

    pub fn new_kernel_thread(entry_point: extern "C" fn()) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        let mut stack = vec![0u8; Self::STACK_SIZE].into_boxed_slice();
        
        let stack_ptr = stack.as_mut_ptr();
        let stack_top = unsafe { stack_ptr.add(Self::STACK_SIZE) as u64 };
        
        // We write a fake Context at the top of the stack.
        let context_size = core::mem::size_of::<Context>() as u64;
        let context_ptr = (stack_top - context_size) as *mut Context;
        
        extern "C" {
            fn thread_entry_stub();
        }
        
        unsafe {
            *context_ptr = Context::empty();
            (*context_ptr).rip = thread_entry_stub as u64;
            (*context_ptr).r12 = entry_point as u64;
        }

        Self {
            id,
            state: SavedThreadState::Cooperative(context_ptr as u64),
            stack: Arc::new(stack),
            cr3: None,
            cspace: Some(Arc::new(spin::Mutex::new(CSpace::new()))),
            process_state: None,
        }
    }

    pub fn new_user_thread(entry_point: u64, user_stack: u64, cr3_phys: u64, cspace: Arc<Mutex<CSpace>>, process_state: Arc<Mutex<crate::task::process::ProcessState>>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        let mut stack = vec![0u8; Self::STACK_SIZE].into_boxed_slice();
        let stack_ptr = stack.as_mut_ptr();
        let stack_top = unsafe { stack_ptr.add(Self::STACK_SIZE) as u64 };

        // We push the trampoline arguments (entry_point, user_stack) onto the top of the stack,
        // or we can pass them in registers. Let's pass them in registers rbx, r12 for instance, 
        // since switch_context pops them.
        let context_size = core::mem::size_of::<Context>() as u64;
        let context_ptr = (stack_top - context_size) as *mut Context;
        
        unsafe {
            *context_ptr = Context::empty();
            (*context_ptr).rip = user_trampoline as *const () as u64;
            // Pass arguments to trampoline via callee-saved registers
            (*context_ptr).r12 = entry_point;
            (*context_ptr).r13 = user_stack;
        }

        Self {
            id,
            state: SavedThreadState::Cooperative(context_ptr as u64),
            stack: Arc::new(stack),
            cr3: Some(cr3_phys),
            cspace: Some(cspace),
            process_state: Some(process_state),
        }
    }
}

extern "C" {
    fn enter_user(trap_frame: *const InterruptContext);
    fn user_trampoline();
}

#[no_mangle]
pub extern "C" fn user_trampoline_rust(entry_point: u64, user_stack: u64) {
    let selectors = crate::arch::x86_64::gdt::selectors();
    
    // Create the hardware interrupt frame
    let trap_frame = InterruptContext {
        r15: 0, r14: 0, r13: 0, r12: 0, r11: 0, r10: 0, r9: 0, r8: 0,
        rdi: 0, rsi: 0, rbp: 0, rbx: 0, rdx: 0, rcx: 0, rax: 0,
        rip: entry_point,
        cs: selectors.user_code_selector.0 as u64,
        rflags: 0x202, // Interrupts enabled
        rsp: user_stack,
        ss: selectors.user_data_selector.0 as u64,
    };
    
    unsafe {
        enter_user(&trap_frame);
    }
}

pub struct Scheduler {
    pub run_queue: VecDeque<alloc::boxed::Box<Thread>>,
    pub current: Option<alloc::boxed::Box<Thread>>,
    pub irq_waiters: [Option<alloc::boxed::Box<Thread>>; 256],
    pub pending_irqs: [bool; 256],
}

impl Scheduler {
    pub const fn new() -> Self {
        const INIT_WAITERS: Option<alloc::boxed::Box<Thread>> = None;
        Self {
            run_queue: VecDeque::new(),
            current: None,
            irq_waiters: [INIT_WAITERS; 256],
            pending_irqs: [false; 256],
        }
    }

    pub fn add_thread(&mut self, thread: Thread) {
        self.run_queue.push_back(alloc::boxed::Box::new(thread));
    }
    
    pub fn prepare_thread_switch(&self, next_thread: &Thread) {
        let stack_top = next_thread.stack.as_ptr() as u64 + next_thread.stack.len() as u64;
        crate::arch::x86_64::gdt::set_tss_rsp0(stack_top);
        crate::arch::x86_64::cpu::set_kernel_stack(stack_top);
        
        if let Some(new_cr3) = next_thread.cr3 {
            let (current_cr3, _) = x86_64::registers::control::Cr3::read();
            if current_cr3.start_address().as_u64() != new_cr3 {
                let frame = x86_64::structures::paging::PhysFrame::from_start_address(x86_64::PhysAddr::new(new_cr3)).unwrap();
                unsafe {
                    x86_64::registers::control::Cr3::write(frame, x86_64::registers::control::Cr3Flags::empty());
                }
            }
        }
    }
    
    // Internal method to switch to next thread.
    // Must be called with interrupts disabled.
    fn pick_next(&mut self, saved_state: SavedThreadState) -> SavedThreadState {
        // Save the current thread's state
        if let Some(mut current) = self.current.take() {
            current.state = saved_state;
            self.run_queue.push_back(current);
        }

        // Get the next thread
        if let Some(next) = self.run_queue.pop_front() {
            let next_state = next.state;
            
            self.prepare_thread_switch(&next);
            
            self.current = Some(next);
            next_state
        } else {
            // If there's no next thread, just return the current state to continue running
            saved_state
        }
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

// Called by the naked timer interrupt handler.
// Returns a tuple of (tag, rsp) to instruct the assembly on how to restore.
// 0 = Cooperative, 1 = Preemptive
#[repr(C)]
pub struct SchedulerReturn {
    pub tag: u64,
    pub rsp: u64,
}

#[no_mangle]
pub extern "C" fn timer_tick_handler(rsp: u64) -> SchedulerReturn {
    let mut scheduler = SCHEDULER.lock();
    let current_state = SavedThreadState::Preemptive(rsp);
    let next_state = scheduler.pick_next(current_state);
    
    // Acknowledge LAPIC timer interrupt so it doesn't block other interrupts
    crate::arch::x86_64::apic::eoi();
    
    match next_state {
        SavedThreadState::Cooperative(sp) => SchedulerReturn { tag: 0, rsp: sp },
        SavedThreadState::Preemptive(sp) => SchedulerReturn { tag: 1, rsp: sp },
    }
}

static mut STATE_TO_UPDATE: *mut SavedThreadState = core::ptr::null_mut();

// Helper to handle the return from switch_context
#[no_mangle]
pub extern "C" fn after_switch(old_rsp: u64) {
    unsafe {
        if !STATE_TO_UPDATE.is_null() {
            *STATE_TO_UPDATE = SavedThreadState::Cooperative(old_rsp);
            STATE_TO_UPDATE = core::ptr::null_mut();
        }
    }
}

pub fn exit_current_thread() -> ! {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        
        let thread_id = scheduler.current.as_ref().unwrap().id;
        log::info!("[SCHEDULER] Thread {} exited.", thread_id);
        
        // Mark process as terminated and wake up waiting threads
        let process_state_opt = scheduler.current.as_ref().unwrap().process_state.clone();
        if let Some(state_arc) = process_state_opt {
            let mut state = state_arc.lock();
            state.terminated = true;
            while let Some(waiting_thread) = state.wait_queue.pop_front() {
                scheduler.run_queue.push_back(alloc::boxed::Box::new(waiting_thread));
            }
        }
        
        scheduler.current = None;
        
        if let Some(next_thread) = scheduler.run_queue.pop_front() {
            let next_state = next_thread.state;
            scheduler.prepare_thread_switch(&next_thread);
            scheduler.current = Some(next_thread);
            drop(scheduler);
            
            match next_state {
                SavedThreadState::Cooperative(new_rsp) => {
                    unsafe { crate::task::switch::switch_context_to_cooperative(new_rsp) };
                    unreachable!("Thread exited, but somehow returned");
                },
                SavedThreadState::Preemptive(new_rsp) => {
                    unsafe { crate::task::switch::switch_context_to_preemptive(new_rsp) };
                    unreachable!("Thread exited, but somehow returned");
                }
            }
        } else {
            panic!("All threads exited. System halted.");
        }
    });
    unreachable!()
}

pub fn sys_wait(handle: usize) -> isize {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        
        let process_state_arc = {
            let current_thread = scheduler.current.as_ref().unwrap();
            let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
            let cspace = cspace_arc.lock();
            let cap = cspace.get(handle);
            match cap {
                Some(crate::capability::Capability::Process(state, rights)) => {
                    if !rights.contains(crate::capability::Rights::WAIT) {
                        log::info!("[KERNEL] sys_wait({}) failed: missing WAIT right", handle);
                        return -3; // PermissionDenied
                    }
                    state
                },
                other => {
                    log::info!("[KERNEL] sys_wait({}) failed: WrongType or InvalidHandle, got {:?}", handle, other.is_some());
                    return -2; // InvalidHandle or WrongType
                }
            }
        };
        
        let mut process_state = process_state_arc.lock();
        if process_state.terminated {
            return 0; // Already terminated
        }
        
        // Block current thread
        let current_thread = scheduler.current.take().unwrap();
        process_state.wait_queue.push_back(*current_thread);
        
        let back_idx = process_state.wait_queue.len() - 1;
        let state_ptr = &mut process_state.wait_queue[back_idx].state as *mut SavedThreadState;
        
        unsafe {
            STATE_TO_UPDATE = state_ptr;
        }
        
        drop(process_state);
        
        while scheduler.run_queue.is_empty() {
            drop(scheduler);
            x86_64::instructions::interrupts::enable_and_hlt();
            x86_64::instructions::interrupts::disable();
            scheduler = SCHEDULER.lock();
        }
        
        let next_thread = scheduler.run_queue.pop_front().unwrap();
        let next_state = next_thread.state;
        scheduler.prepare_thread_switch(&next_thread);
        scheduler.current = Some(next_thread);
        
        drop(scheduler);
        
        match next_state {
            SavedThreadState::Cooperative(new_rsp) => {
                let old_rsp = unsafe { switch_context(new_rsp) };
                after_switch(old_rsp);
            },
            SavedThreadState::Preemptive(new_rsp) => {
                unsafe { switch_context_to_preemptive(new_rsp) };
            }
        }
        
        0 // Successfully waited
    })
}

// Yields the current thread cooperatively.
pub fn yield_now() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        
        if scheduler.run_queue.is_empty() {
            return;
        }
        
        if let Some(current_thread) = scheduler.current.take() {
            scheduler.run_queue.push_back(current_thread);
            let back_idx = scheduler.run_queue.len() - 1;
            let state_ptr = &mut scheduler.run_queue[back_idx].state as *mut SavedThreadState;
            unsafe {
                STATE_TO_UPDATE = state_ptr;
            }
        } else {
            unsafe {
                STATE_TO_UPDATE = core::ptr::null_mut();
            }
        }
        
        let next_thread = scheduler.run_queue.pop_front().unwrap();
        let next_state = next_thread.state;
        
        scheduler.prepare_thread_switch(&next_thread);
        
        scheduler.current = Some(next_thread);
        
        drop(scheduler);
        
        match next_state {
            SavedThreadState::Cooperative(new_rsp) => {
                let old_rsp = unsafe { switch_context(new_rsp) };
                after_switch(old_rsp);
            },
            SavedThreadState::Preemptive(_) => {
                unimplemented!("Cooperative yield to Preemptive thread not supported in Phase 4/5. Wait for timer!");
            }
        }
    });
}

pub fn sys_send(handle: usize, msg: crate::ipc::Message) -> Result<(), crate::capability::CapError> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        
        let current_thread = scheduler.current.as_ref().unwrap();
        let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
        
        let endpoint_arc = {
            let cspace = cspace_arc.lock();
            let cap = cspace.get(handle).ok_or(crate::capability::CapError::InvalidHandle)?;
            match cap {
                crate::capability::Capability::Endpoint(ep, rights) => {
                    if !rights.contains(crate::capability::Rights::SEND) {
                        return Err(crate::capability::CapError::PermissionDenied);
                    }
                    ep
                }
                crate::capability::Capability::Process(_, _) => return Err(crate::capability::CapError::WrongType),
                _ => return Err(crate::capability::CapError::WrongType),
            }
        };
        
        // We have the endpoint.
        let mut ep_state = endpoint_arc.state.lock();
        
        match &mut *ep_state {
            crate::ipc::EndpointState::ReceiversWaiting(queue) => {
                if let Some((receiver, msg_ptr)) = queue.pop_front() {
                    // We can immediately send the message!
                    // Write the message into the receiver's memory buffer.
                    unsafe {
                        *(msg_ptr.as_mut()) = msg;
                    }
                    
                    // Put the receiver back on the run queue.
                    scheduler.run_queue.push_back(receiver);
                    return Ok(());
                }
            }
            _ => {} // Fall through to blocking
        }
        
        // If we reach here, we must block.
        // Convert Idle to SendersWaiting if necessary
        if let crate::ipc::EndpointState::Idle = &*ep_state {
            *ep_state = crate::ipc::EndpointState::SendersWaiting(alloc::collections::VecDeque::new());
        }
        
        if let crate::ipc::EndpointState::SendersWaiting(queue) = &mut *ep_state {
            let current_thread = scheduler.current.take().unwrap();
            queue.push_back((current_thread, msg));
            
            let back_idx = queue.len() - 1;
            let state_ptr = &mut queue[back_idx].0.state as *mut SavedThreadState;
            
            unsafe {
                STATE_TO_UPDATE = state_ptr;
            }
            
            drop(ep_state);
            
            while scheduler.run_queue.is_empty() {
                drop(scheduler);
                x86_64::instructions::interrupts::enable_and_hlt();
                x86_64::instructions::interrupts::disable();
                scheduler = SCHEDULER.lock();
            }
            
            let next_thread = scheduler.run_queue.pop_front().unwrap();
            let next_state = next_thread.state;
            scheduler.prepare_thread_switch(&next_thread);
            scheduler.current = Some(next_thread);
            drop(scheduler);
            
            match next_state {
                SavedThreadState::Cooperative(new_rsp) => {
                    let old_rsp = unsafe { switch_context(new_rsp) };
                    after_switch(old_rsp);
                },
                SavedThreadState::Preemptive(new_rsp) => {
                    unsafe { switch_context_to_preemptive(new_rsp) };
                }
            }
        }
        
        Ok(())
    })
}

pub fn sys_recv(handle: usize) -> Result<crate::ipc::Message, crate::capability::CapError> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        
        let current_thread = scheduler.current.as_ref().unwrap();
        let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
        
        let endpoint_arc = {
            let cspace = cspace_arc.lock();
            let cap = cspace.get(handle).ok_or(crate::capability::CapError::InvalidHandle)?;
            match cap {
                crate::capability::Capability::Endpoint(ep, rights) => {
                    if !rights.contains(crate::capability::Rights::RECEIVE) {
                        return Err(crate::capability::CapError::PermissionDenied);
                    }
                    ep
                }
                crate::capability::Capability::Process(_, _) => return Err(crate::capability::CapError::WrongType),
                _ => return Err(crate::capability::CapError::WrongType),
            }
        };
        
        // We need a local buffer for the message if we block.
        let mut msg_buf = crate::ipc::Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
        
        let mut ep_state = endpoint_arc.state.lock();
        
        match &mut *ep_state {
            crate::ipc::EndpointState::SendersWaiting(queue) => {
                if let Some((sender, msg)) = queue.pop_front() {
                    // We received a message immediately!
                    // Put the sender back on the run queue.
                    scheduler.run_queue.push_back(sender);
                    return Ok(msg);
                }
            }
            _ => {} // Fall through to blocking
        }
        
        // If we reach here, we must block.
        if let crate::ipc::EndpointState::Idle = &*ep_state {
            *ep_state = crate::ipc::EndpointState::ReceiversWaiting(alloc::collections::VecDeque::new());
        }
        
        if let crate::ipc::EndpointState::ReceiversWaiting(queue) = &mut *ep_state {
            let current_thread = scheduler.current.take().unwrap();
            let msg_ptr = crate::ipc::MessagePtr::new(&mut msg_buf as *mut crate::ipc::Message);
            queue.push_back((current_thread, msg_ptr));
            
            let back_idx = queue.len() - 1;
            let state_ptr = &mut queue[back_idx].0.state as *mut SavedThreadState;
            
            unsafe {
                STATE_TO_UPDATE = state_ptr;
            }
            
            drop(ep_state);
            
            while scheduler.run_queue.is_empty() {
                drop(scheduler);
                x86_64::instructions::interrupts::enable_and_hlt();
                x86_64::instructions::interrupts::disable();
                scheduler = SCHEDULER.lock();
            }
            
            let next_thread = scheduler.run_queue.pop_front().unwrap();
            let next_state = next_thread.state;
            scheduler.prepare_thread_switch(&next_thread);
            scheduler.current = Some(next_thread);
            drop(scheduler);
            
            match next_state {
                SavedThreadState::Cooperative(new_rsp) => {
                    let old_rsp = unsafe { switch_context(new_rsp) };
                    after_switch(old_rsp);
                },
                SavedThreadState::Preemptive(new_rsp) => {
                    unsafe { switch_context_to_preemptive(new_rsp) };
                }
            }
        }
        
        // When we wake up, the message has been written into msg_buf!
        Ok(msg_buf)
    })
}

pub fn sys_endpoint_create() -> usize {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        let current_thread = scheduler.current.as_ref().unwrap();
        let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
        
        let ep = alloc::sync::Arc::new(crate::ipc::Endpoint::new());
        let cap = crate::capability::Capability::Endpoint(ep, crate::capability::Rights::ALL);
        
        let mut cspace = cspace_arc.lock();
        cspace.insert(cap)
    })
}

pub fn unblock_irq(irq: u8) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        if let Some(thread) = scheduler.irq_waiters[irq as usize].take() {
            scheduler.run_queue.push_back(thread);
        } else {
            scheduler.pending_irqs[irq as usize] = true;
        }
    });
}

pub fn sys_wait_irq(handle: usize) -> isize {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        
        let target_irq = {
            let current_thread = scheduler.current.as_ref().unwrap();
            let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
            let cspace = cspace_arc.lock();
            let cap = cspace.get(handle);
            match cap {
                Some(crate::capability::Capability::Interrupt(irq)) => irq,
                _ => return -2, // InvalidHandle or WrongType
            }
        };
        // If the IRQ is already pending, consume it and return immediately
        if scheduler.pending_irqs[target_irq as usize] {
            scheduler.pending_irqs[target_irq as usize] = false;
            return 0;
        }
        
        // Block current thread
        let current_thread = scheduler.current.take().unwrap();
        scheduler.irq_waiters[target_irq as usize] = Some(current_thread);
        
        let state_ptr = &mut scheduler.irq_waiters[target_irq as usize].as_mut().unwrap().state as *mut SavedThreadState;
        
        unsafe {
            STATE_TO_UPDATE = state_ptr;
        }
        
        while scheduler.run_queue.is_empty() {
            drop(scheduler);
            x86_64::instructions::interrupts::enable_and_hlt();
            x86_64::instructions::interrupts::disable();
            scheduler = SCHEDULER.lock();
        }
        
        let next_thread = scheduler.run_queue.pop_front().unwrap();
        let next_state = next_thread.state;
        scheduler.prepare_thread_switch(&next_thread);
        scheduler.current = Some(next_thread);
        drop(scheduler);
        
        match next_state {
            SavedThreadState::Cooperative(new_rsp) => {
                let old_rsp = unsafe { switch_context(new_rsp) };
                after_switch(old_rsp);
            },
            SavedThreadState::Preemptive(new_rsp) => {
                unsafe { switch_context_to_preemptive(new_rsp) };
            }
        }
        
        0
    })
}
