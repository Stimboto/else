.global switch_context
switch_context:
    // rdi contains new_stack (rsp of the new thread context)
    // we need to save the callee-saved registers of the CURRENT thread.
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    
    // save the current rsp to return it (in rax)
    mov rax, rsp
    
    // switch to the new stack
    mov rsp, rdi
    
    // restore the callee-saved registers of the NEW thread.
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

.global switch_context_to_preemptive
switch_context_to_preemptive:
    // rdi contains new_stack (rsp of the new PREEMPTIVE thread context, which is an InterruptContext)
    // save the callee-saved registers of the CURRENT thread.
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    
    // save old_rsp and new_stack
    mov r8, rsp
    mov r9, rdi
    
    mov rdi, r8  // arg 1 = old_rsp
    
    // Align stack to 16 bytes for C ABI
    mov r12, rsp
    and rsp, -16
    
    call after_switch
    
    mov rsp, r12
    mov rdi, r9
    jmp enter_user


.global thread_entry_stub
thread_entry_stub:
    // switch_context returns the old thread's rsp in rax
    mov rdi, rax
    // call after_switch(old_rsp) to finish the context switch in Rust
    call after_switch
    
    // now we can safely call the actual thread payload
    // the payload address was stored in r12 when the context was initialized
    call r12
    
.L_thread_exit:
    cli
    hlt
    jmp .L_thread_exit

.global enter_user
enter_user:
    // rdi contains a pointer to the InterruptContext
    // we need to load this context as if we were returning from an interrupt
    mov rsp, rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    // If returning to User Mode, we must swapgs!
    // We check CS at [rsp + 8]
    cmp qword ptr [rsp + 8], 0x8
    je 1f
    swapgs
1:
    // IRETQ uses: rip, cs, rflags, rsp, ss
    iretq

.global user_trampoline
user_trampoline:
    // when a user thread is created, it starts here with:
    // rax = old_rsp (returned from switch_context)
    // r12 = user_entry_point
    // r13 = user_stack
    
    // 1. Finish the context switch by saving the old thread's RSP
    mov rdi, rax
    call after_switch
    
    // 2. Set up arguments for user_trampoline_rust
    mov rdi, r12
    mov rsi, r13
    call user_trampoline_rust
.L_user_trampoline_end:
    cli
    hlt
    jmp .L_user_trampoline_end

.global timer_interrupt_stub
timer_interrupt_stub:
    // Check if coming from User Mode (CS != 0x8)
    // RSP points to RIP, RSP+8 points to CS
    cmp qword ptr [rsp + 8], 0x8
    je 1f
    swapgs
1:

    // Save InterruptContext
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    
    mov rdi, rsp
    call timer_tick_handler
    
    // Result is in RAX (tag) and RDX (rsp)
    mov rbx, rax
    mov rcx, rdx
    
    // Send EOI to PIC (IRQ 0 is on Master PIC)
    mov al, 0x20
    out 0x20, al
    
    cmp rbx, 0
    je .L_resume_cooperative
    
    // Resume preemptive
    mov rsp, rcx
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    
    cmp qword ptr [rsp + 8], 0x8
    je 2f
    swapgs
2:
    iretq
    
.L_resume_cooperative:
    // Switch to a cooperative stack frame
    // Cooperative context is just the callee-saved registers pushed by switch_context.
    mov rsp, rcx
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    
    // A cooperative return means we are returning to kernel code that yielded!
    // Since we are returning to Kernel Mode, and we checked swapgs on entry,
    // if we entered from User Mode we swapped to Kernel GS. 
    // Wait, if we return to a cooperative kernel thread, its CS is implicitly 0x8!
    // We don't iretq here, we ret!
    // So we don't swapgs back! The GS base remains Kernel GS Base.
    // This perfectly handles the swapgs logic!
    ret

.global switch_context_to_cooperative
switch_context_to_cooperative:
    mov rsp, rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
