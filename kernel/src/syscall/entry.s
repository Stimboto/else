.global syscall_entry
syscall_entry:
    // SYSCALL enters here with:
    // RIP = address of syscall_entry
    // RFLAGS = saved in R11 (and masked by FMASK)
    // CS and SS loaded from STAR
    // USER RIP saved in RCX
    // USER RSP is still in RSP
    // GS base is still USER GS BASE
    
    // 1. Swap GS to access kernel per-cpu data
    swapgs
    
    // 2. Safely swap RSP to kernel stack
    // Save user RSP into PerCpu struct (offset 8 since kernel_stack is at offset 0, wait let's check!)
    // PerCpu has kernel_stack at offset 0, user_stack at offset 8.
    mov gs:[8], rsp
    
    // Load kernel RSP from PerCpu struct
    mov rsp, gs:[0]
    
    // 3. We are now on a safe kernel stack. Save user registers.
    // We will push registers to create a context that we can restore from.
    // SYSRET requires:
    // RCX = USER RIP
    // R11 = USER RFLAGS
    // The syscall arguments are in RDI, RSI, RDX, R10, R8, R9.
    // The syscall number is in RAX.
    
    // Push the state in a way that aligns with our needs, or just push everything.
    // We need to preserve callee-saved registers as well, in case we block and context switch!
    // If we call a Rust function, it will save its own callee-saved registers. 
    // BUT we must save ALL user registers because SYSRET returns to user space.
    // Callee-saved in user space might be clobbered by kernel Rust code!
    // Let's just save all GPRs.
    
    push r15
    push r14
    push r13
    push r12
    push r11  // RFLAGS
    push r10
    push r9
    push r8
    push rdi
    push rsi
    push rbp
    push rbx
    push rdx
    push rcx  // USER RIP
    push rax  // Syscall number
    
    // Save the GS-saved user_rsp just to be complete, or we can restore it from GS later.
    // Let's push user RSP so the stack frame has everything.
    mov rbx, gs:[8]
    push rbx  // USER RSP
    
    // 4. Call Rust handler
    // Arguments are already in RDI, RSI, RDX, R10, R8, R9, RAX.
    // Wait, RAX is the 7th argument? SysV AMD64 ABI passes args in: RDI, RSI, RDX, RCX, R8, R9.
    // Rust handler signature: (rdi: u64, rsi: u64, rdx: u64, r10: u64, r8: u64, r9: u64, rax: u64)
    // In SysV AMD64:
    // Arg 1 = RDI (matches)
    // Arg 2 = RSI (matches)
    // Arg 3 = RDX (matches)
    // Arg 4 = RCX (but we use R10, so we need to move R10 to RCX)
    // Arg 5 = R8 (matches)
    // Arg 6 = R9 (matches)
    // Arg 7 = stack (RAX needs to be passed on stack)
    
    // Let's move args around for the C ABI:
    mov rcx, r10 // Arg 4
    
    // Ensure 16-byte stack alignment BEFORE the call instruction.
    // SysV ABI requires RSP % 16 == 0 before `call`.
    mov rbp, rsp
    
    // We need to push 1 argument (8 bytes) before the call.
    // To make RSP % 16 == 0 before the call, we must make RSP % 16 == 8 before the push.
    and rsp, -16
    sub rsp, 8
    
    // Now push Arg 7 (RAX) onto the stack for the call.
    push rax
    
    call syscall_handler
    
    // Restore RSP to what it was before we aligned it
    mov rsp, rbp
    
    // Syscall handler returned the result in RAX. We need to preserve RAX across the pop sequence.
    // We pushed: user_rsp, rax, rcx, rdx, rbx, rbp, rsi, rdi, r8, r9, r10, r11, r12, r13, r14, r15
    
    // Restore registers
    pop rbx     // Pop user RSP into RBX
    mov gs:[8], rbx // Save it back to PerCpu just in case
    
    add rsp, 8  // Skip saved RAX (we want to keep the new RAX from the handler)
    
    // Validate that RCX (user RIP) is a canonical user-space address (< 0x0000800000000000)
    // RCX is currently at [rsp]. We use r11 as a scratch register because it will be restored later.
    mov r11, [rsp]
    shr r11, 47
    test r11, r11
    jnz .L_sysret_invalid_rip

    pop rcx     // USER RIP
    pop rdx
    pop rbx
    pop rbp
    pop rsi
    pop rdi
    pop r8
    pop r9
    pop r10
    pop r11     // USER RFLAGS
    pop r12
    pop r13
    pop r14
    pop r15
    
    // Restore user RSP
    mov rsp, gs:[8]
    
    // Swap GS back to user GS
    swapgs
    
    // Return to user mode!
    sysretq
    
.L_sysret_invalid_rip:
    // If RIP is invalid, we cannot return safely. 
    // In a real OS, we would send a SIGSEGV. Here, we just halt.
    cli
.L_halt_loop:
    hlt
    jmp .L_halt_loop

