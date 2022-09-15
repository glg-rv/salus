// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use elf_rs::Elf;

use page_tracking::HypPageAlloc;
use riscv_page_tables::{FirstStagePageTable, PteFieldBits, PteLeafPerms, Sv48};
use riscv_pages::{PageAddr, PageSize, PhysPage, RawAddr};

use crate::smp::PerCpu;
use crate::task::Task;

/// Loads the task
pub fn load(alloc: &mut HypPageAlloc) -> Option<Task> {
    let u_pages = alloc.take_pages_for_host_state_with_alignment(12, 4096);
    let allocated_pte_pages = alloc.take_pages_for_host_state_with_alignment(4, 4096);
    let mut pte_pages = allocated_pte_pages.into_iter();
    let root_page = pte_pages.next().unwrap();
    let page_table: FirstStagePageTable<Sv48> =
        FirstStagePageTable::new(root_page.into()).expect("creating sv48");

    let gpa_base = PageAddr::new(RawAddr::supervisor_virt(0x8000_0000)).unwrap();
    let pte_fields = PteFieldBits::leaf_with_perms(PteLeafPerms::RWX);
    let mapper = page_table
        .map_range(gpa_base, PageSize::Size4k, 2, &mut || pte_pages.next())
        .unwrap();
    for (page, gpa) in u_pages.into_iter().zip(gpa_base.iter_from()) {
        unsafe {
            // safe to map the page as it will be given to the task while it's running.
            // s-mode won't hold any references to the page or data it contains.
            mapper.map_4k_addr(gpa, page.addr(), pte_fields).unwrap();
        }
    }

    // load the code
    let bytes = include_bytes!("../target/riscv64gc-unknown-none-elf/release/umode");
    let elf = Elf::from_bytes(bytes).unwrap(); // TODO

    let elf64 = match elf {
        Elf::Elf64(elf) => elf,
        _ => panic!("got Elf32, expected Elf64"),
    };
    for _header in elf64.program_header_iter() {
        // TODO
    }
    Some(Task::new(page_table))
}
