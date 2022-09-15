// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use elf_rs::Elf;

use riscv_page_tables::{FirstStagePageTable, Sv48};
use riscv_pages::{Page, PageSize, PhysPage};

use crate::smp::PerCpu;
use crate::task::Task;

/// Loads the task
pub fn load() -> Option<Task> {
    let (user_start, num_user_pages) = PerCpu::this_cpu().user_mode_range();
    // Safety: page is uniquely owned after being taken from those resered for this purpose.
    let root_page = unsafe { Page::new_with_size(user_start, PageSize::Size4k) };
    let page_table: FirstStagePageTable<Sv48> =
        FirstStagePageTable::new(root_page.into()).expect("creating sv48");

    let bytes = include_bytes!("../target/riscv64gc-unknown-none-elf/release/memcpy");
    let elf = Elf::from_bytes(bytes).unwrap(); // TODO

    let elf64 = match elf {
        Elf::Elf64(elf) => elf,
        _ => panic!("got Elf32, expected Elf64"),
    };
    let (page_start, num_pages) = PerCpu::this_cpu().user_mode_range();
    for header in elf64.program_header_iter() {
        // TODO
    }
    None
}
