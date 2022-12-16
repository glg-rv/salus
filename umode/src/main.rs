// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]

extern crate libuser;

use libuser::*;

static mut a: i32 = 5;

#[no_mangle]
extern "C" fn task_main(_data: u64) {
    println!("----------------------------");
    println!("{}", a);
    unsafe {
        a += 1;
        a += 1;
    }
    let ptr = unsafe { &mut a as *mut i32 };
    unsafe {
        *ptr = 4;
    }
    println!(" ___________________");
    println!("< Hello from UMODE! >");
    println!(" -------------------");
    println!("        \\   ^__^");
    println!("         \\  (oo)\\_______");
    println!("            (__)\\       )\\/\\");
    println!("                ||----w |");
    println!("                ||     ||");
    println!("----------------------------");
    println!("{} {:#?} ", a, ptr);
    panic!("");
}
