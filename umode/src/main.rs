// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]

extern crate libuser;

use libuser::hypcalls::*;
use libuser::*;
use umode_api::{Error as UmodeApiError, UmodeOp, UmodeRequest};

static mut a: i32 = 5;

#[no_mangle]
extern "C" fn task_main(initial_request: Result<UmodeRequest, UmodeApiError>) {
    println!("Umode started.");
    let mut req = initial_request;
    loop {
        let res = match req {
            Ok(req) => match req.op() {
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
                UmodeOp::Nop => {
                    println!("Nop");
                    Ok(())
                }
            },
            Err(err) => Err(err),
        };
        // Return result and wait for next operation.
        req = hyp_nextop(res)
    }
    panic!("Loop exited.");
}
