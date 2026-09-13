use core::arch::global_asm;
use super::context::InterruptContext;

global_asm!(include_str!("switch.s"));

extern "C" {
    pub fn switch_context(new_rsp: u64) -> u64;
    pub fn switch_context_to_preemptive(new_rsp: u64);
    pub fn switch_context_to_cooperative(new_rsp: u64);
    pub fn thread_entry_stub();
    pub fn enter_user(trap_frame: *const InterruptContext);
    pub fn user_trampoline();
    pub fn timer_interrupt_stub();
}
