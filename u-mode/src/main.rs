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

use data_model::{VolatileMemory, VolatileSlice};
use libuser::*;
use u_mode_api::{Error as UmodeApiError, UmodeOp, UmodeRequest};

struct UmodeTask {
    vslice: VolatileSlice<'static>,
}

impl UmodeTask {
    // (Test) Print String from U-mode Mapped Area
    //
    // Arguments:
    //    [0] = length of data in the  to be printed.
    //
    // U-mode Mapped Area:
    //    Contains the data to be printed at the beginning of the area.
    fn op_print_string(&self, req: &UmodeRequest) -> Result<(), UmodeApiError> {
        // Print maximum 80 chars.
        const max_length: usize = 80;
        let len = req.args[0] as usize;
        let vs_input = self.vslice.get_slice(0, len).map_err(|_| UmodeApiError::InvalidArgument)?;
        // Copy input from volatile slice.
        let mut input = [0u8; max_length];
        vs_input.copy_to(&mut input[..]);
        let len = core::cmp::min(max_length, len);
        println!(
            "Received a {} bytes string: \"{}\"", len,
            core::str::from_utf8(&input[0..len]).map_err(|_| UmodeApiError::InvalidArgument)?
        );
        /*
                println!(
                    "{}",
                    core::str::from_utf8(input).map_err(|_| UmodeApiError::InvalidArgument)?
                );
        */
        Ok(())
    }

    // Copy memory from input to output.
    //
    // Arguments:
    //    [0] = starting address of output
    //    [1] = starting address of input
    //    [2] = length of input and output
    //
    // U-mode Mapped Area: Not used.
    fn op_memcopy(&self, req: &UmodeRequest) -> Result<(), UmodeApiError> {
        let out_addr = req.args[0];
        let in_addr = req.args[1];
        let len = req.args[2] as usize;
        // Safety: we trust the hypervisor to have mapped at `in_addr` `len` bytes for reading.
        let input = unsafe { &*core::ptr::slice_from_raw_parts(in_addr as *const u8, len) };
        // Safety: we trust the hypervisor to have mapped at `out_addr` `len` bytes valid
        // for reading and writing.
        let output = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(out_addr as *mut u8, len) };
        output[0..len].copy_from_slice(&input[0..len]);
        Ok(())
    }

    // Run the main loop, receiving requests from the hypervisor and executing them.
    fn run_loop(&self) -> ! {
        let mut res = Ok(());
        loop {
            // Return result and wait for next operation.
            let req = hyp_nextop(res);
            res = match req {
                Ok(req) => match req.op {
                    UmodeOp::Nop => Ok(()),
                    UmodeOp::PrintString => self.op_print_string(&req),
                    UmodeOp::MemCopy => self.op_memcopy(&req),
                    UmodeOp::GetEvidence => Err(UmodeApiError::InvalidArgument),
                },
                Err(err) => Err(err),
            };
        }
    }
}

#[no_mangle]
extern "C" fn task_main(cpuid: u64, uma_addr: u64, uma_size: u64) -> ! {
    // Safety: we trust the hypervisor to have mapped an area of memory starting at `uma_addr` valid
    // for at least `uma_size` bytes.
    let vslice = unsafe { VolatileSlice::from_raw_parts(uma_addr as *mut u8, uma_size as usize) };
    let task = UmodeTask { vslice };
    println!(
        "umode/#{}: U-mode Mapped Area: {:016x} - {} bytes",
        cpuid, uma_addr, uma_size
    );
    task.run_loop();
}
