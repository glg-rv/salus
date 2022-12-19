// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]

pub mod hypcalls;

use crate::hypcalls::*;
use core::arch::global_asm;

global_asm!(include_str!("task_start.S"));

pub struct UserWriter {}

impl core::fmt::Write for UserWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            let _ = hyp_putchar(c);
        }
        Ok(())
    }
}

pub static mut UMODEWRITER: UserWriter = UserWriter {};

#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {
        {
            use core::fmt::Write;
            // TODO: WRITE SAFE REASON
            unsafe {
                write!(&mut UMODEWRITER, $($args)*).unwrap();
            }
        }
    };
}

#[macro_export]
macro_rules! println {
    ($($args:tt)*) => {
        {
            use core::fmt::Write;
            // TODO: WRITE SAFE REASON
            unsafe {
                writeln!(&mut UMODEWRITER, $($args)*).unwrap();
            }
        }
    };
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic : {:?}", info);
    hyp_panic();
    unreachable!()
}

use umode_api::{Error as UmodeApiError, TryIntoRegisters, UmodeRequest};

extern "C" {
    fn task_main(regs: Result<UmodeRequest, UmodeApiError>);
}

// Start from asm. Registers contain an `UmodeRequest`. Decode and call `task_main`.
#[no_mangle]
extern "C" fn _libuser_start(ptr: *mut u64, len: u64) {
    // Safety: We trust the hypervisor to have called us with the
    // registers in a0-a7. The assembly code in task_start.S has moved
    // them to an array and passed address and length of this array.
    let mut args;
    unsafe {
        args = core::slice::from_raw_parts(ptr as *mut u64, len as usize);
    }
    // Safety: This function is define in umode/src/main.rs.
    unsafe {
        task_main(UmodeRequest::try_from_registers(args));
    }
}
