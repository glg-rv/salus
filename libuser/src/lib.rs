// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]

mod hypcalls;

use crate::hypcalls::*;
use core::arch::global_asm;

global_asm!(include_str!("task_start.S"));

pub struct UserWriter {}

impl core::fmt::Write for UserWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            hyp_putchar(c);
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

// Loop making ecalls as the kernel will kill the task on an ecall (the only syscall supported is
// `exit`).
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    hyp_panic();
    unreachable!()
}
