// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use core::arch::asm;
use umode_api::Error as UmodeApiError;
use umode_api::{HypCall, IntoRegisters, TryIntoRegisters, UmodeRequest};

/// Send an ecall to the hypervisor.
///
/// # Safety
///
/// The caller must verify that any memory references contained in `hypc` obey Rust's memory
/// safety rules. For example, any pointers to memory that will be modified in the handling of
/// the ecall must be uniquely owned. Similarly any pointers read by the ecall must not be
/// mutably borrowed.
unsafe fn ecall(regs: &mut [u64; 8]) {
    asm!("ecall",
         inlateout("a0") regs[0],
         inlateout("a1") regs[1],
         in("a2")regs[2], in("a3") regs[3],
         in("a4")regs[4], in("a5") regs[5],
         in("a6")regs[6], in("a7") regs[7], options(nostack));
}

/// Print a character.
pub fn hyp_putchar(c: char) -> Result<(), UmodeApiError> {
    let mut regs = [0u64; 8];
    let hypc = HypCall::PutChar(c as u8);
    hypc.set_registers(&mut regs);
    // Safety: This ecall does not contain any memory reference.
    unsafe {
        ecall(&mut regs);
    }
    Result::from_registers(&regs)
}

/// Panic and exit immediately.
pub fn hyp_panic() {
    let mut regs = [0u64; 8];
    let hypc = HypCall::Panic;
    hypc.set_registers(&mut regs);
    // Safety: This ecall does not contain any memory reference.
    unsafe {
        ecall(&mut regs);
    }
    unreachable!();
}

pub fn hyp_nextop(result: Result<(), UmodeApiError>) -> Result<UmodeRequest, UmodeApiError> {
    let mut regs = [0u64; 8];
    let hypc = HypCall::NextOp(result);
    hypc.set_registers(&mut regs);
    // Safety: This ecall does not contain any memory reference.
    unsafe {
        ecall(&mut regs);
    }
    // In case there's an error return the error. The caller might decide the error immediately and
    // wait for another request, or panic.
    UmodeRequest::try_from_registers(&regs)
}
