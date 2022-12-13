// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use core::arch::asm;
use umode_api::hypcall::*;

/// Send an ecall to the hypervisor.
///
/// # Safety
///
/// The caller must verify that any memory references contained in `hypc` obey Rust's memory
/// safety rules. For example, any pointers to memory that will be modified in the handling of
/// the ecall must be uniquely owned. Similarly any pointers read by the ecall must not be
/// mutably borrowed.
///
/// In addition the caller is placing trust in the firmware or hypervisor to maintain the promises
/// of the interface w.r.t. reading and writing only within the provided bounds.
pub unsafe fn hyp_call(hypc: &HypCall) -> Result<u64, HypCallError> {
    let mut args = [0u64; 7];
    hypc.to_regs(&mut args);
    asm!("ecall", inlateout("a0") args[0], inlateout("a1") args[1],
                in("a2")args[2], in("a3") args[3],
                in("a4")args[4], in("a5") args[5],
                in("a6")args[6], in("a7") args[7], options(nostack));

    HypReturn::from_regs(&args).into()
}

pub fn hyp_putchar(c: char) {
    let hypc = HypCall::Base(BaseExtension::PutChar(c as u8));
}
