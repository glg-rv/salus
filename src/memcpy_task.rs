// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use elf_rs::Elf;

/// Loads and runs the task
pub fn runit() {
    let bytes = include_bytes!("../target/riscv64gc-unknown-none-elf/release/memcpy");
    let elf = Elf::from_bytes(bytes).unwrap(); // TODO

    let elf64 = match elf {
        Elf::Elf64(elf) => elf,
        _ => panic!("got Elf32, expected Elf64"),
    };
    for header in elf64.program_header_iter() {
        // TODO
    }
}
