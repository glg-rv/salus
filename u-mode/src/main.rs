// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]

extern crate libuser;

use libuser::*;
use u_mode_api::{Error as UmodeApiError, UmodeOp, UmodeRequest};

fn op_memcopy(req: &UmodeRequest) -> Result<(), UmodeApiError> {
    let input = req.input().ok_or(UmodeApiError::InvalidArgument)?;
    let output = req.output().ok_or(UmodeApiError::InvalidArgument)?;
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
            Ok(req) => match req.op() {
                UmodeOp::Nop => Ok(()),
                UmodeOp::Hello => {
                    println!("----------------------------");
                    println!(" ___________________");
                    println!("< Hello from UMODE! >");
                    println!(" -------------------");
                    println!("        \\   ^__^");
                    println!("         \\  (oo)\\_______");
                    println!("            (__)\\       )\\/\\");
                    println!("                ||----w |");
                    println!("                ||     ||");
                    println!("----------------------------");
                    Ok(())
                }
                UmodeOp::MemCopy => op_memcopy(&req),
            },
            Err(err) => Err(err),
        };
    }
}
