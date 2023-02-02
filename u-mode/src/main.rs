// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]

//! # Salus U-mode binary.
//!
//! This is Salus U-mode code. It is used to offload functionalities
//! of the hypervisor in user mode. There's a copy of this task in
//! each CPU.
//!
//! The task it's based on a request loop. This task can be reset at
//! any time by the hypervisor, so it shouldn't hold non-recoverable
//! state.

extern crate libuser;

use libuser::*;
use u_mode_api::{Error as UmodeApiError, UmodeOp, UmodeRequest};

fn op_print_string(req: &UmodeRequest) -> Result<(), UmodeApiError> {
    let in_addr = req.in_addr.ok_or(UmodeApiError::InvalidArgument)?;
    // Safety: we trust the hypervisor to have mapped at `req.in_addr` `req.in_len` bytes for reading.
    let input = unsafe { &*core::ptr::slice_from_raw_parts(in_addr as *const u8, req.in_len) };
    println!(
        "{}",
        core::str::from_utf8(input).map_err(|_| UmodeApiError::InvalidArgument)?
    );
    Ok(())
}

fn op_memcopy(req: &UmodeRequest) -> Result<(), UmodeApiError> {
    let in_addr = req.in_addr.ok_or(UmodeApiError::InvalidArgument)?;
    // Safety: we trust the hypervisor to have mapped at `req.in_addr` `req.in_len` bytes for reading.
    let input = unsafe { &*core::ptr::slice_from_raw_parts(in_addr as *const u8, req.in_len) };
    let out_addr = req.out_addr.ok_or(UmodeApiError::InvalidArgument)?;
    // Safety: we trust the hypervisor to have mapped at `req.out_addr` `req.out_len` bytes valid
    // for reading and writing.
    let output =
        unsafe { &mut *core::ptr::slice_from_raw_parts_mut(out_addr as *mut u8, req.out_len) };
    let len = core::cmp::min(input.len(), output.len());
    output[0..len].copy_from_slice(&input[0..len]);
    Ok(())
}

#[no_mangle]
extern "C" fn task_main(cpuid: u64) -> ! {
    println!("umode/#{} initialized.", cpuid);
    // Initialization done.
    let mut res = Ok(());
    loop {
        // Return result and wait for next operation.
        let req = hyp_nextop(res);
        res = match req {
            Ok(req) => match req.op {
                UmodeOp::Nop => Ok(()),
                UmodeOp::PrintString => op_print_string(&req),
                UmodeOp::MemCopy => op_memcopy(&req),
                UmodeOp::GetEvidence => Err(UmodeApiError::InvalidArgument),
            },
            Err(err) => Err(err),
        };
    }
}
