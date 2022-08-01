// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use elf_rs::Elf;

use page_tracking::{HwMemMap, HwMemRegionType, HwReservedMemType};
use riscv_page_tables::Sv48;
use smp::PerCpu;

/// Loads the task
pub fn load() -> Task {
    let (user_start, num_user_pages) = PerCpu::this_cpu().user_pages(cpu_id);
    // Safety: page is uniquely owned after being taken from those resered for this purpose.
    let page = unsafe { Page::new_with_size(user_start, PageSize::PageSize4k) };
    let pte_pages = Sv48::new(page.into(), PageOwnerId::hypervisor(), phys_pages.clone());

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
}
