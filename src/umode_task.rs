// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use riscv_elf::ElfLoader;

use page_tracking::HypPageAlloc;
use riscv_page_tables::{FirstStagePageTable, PteFieldBits, PteLeafPerms, Sv48};
use riscv_pages::{PageAddr, PageSize, PhysPage, RawAddr};

use crate::smp::PerCpu;
use crate::task::Task;

use s_mode_utils::print::*;

//fn elf_page_count(Elf::Elf64 &elf64) {

//}

/// Loads the task
pub fn load(alloc: &mut HypPageAlloc) -> Option<Task> {
    /* Step 1: Find how many pages we'll have to alloc for the task. */

    /* Step 2: Find how many pages we'll have to alloc for the PTEs. */

    let u_pages = alloc.take_pages_for_host_state_with_alignment(12, 4096);
    let allocated_pte_pages = alloc.take_pages_for_host_state_with_alignment(4, 4096);
    let mut pte_pages = allocated_pte_pages.into_iter();
    let root_page = pte_pages.next().unwrap();
    let page_table: FirstStagePageTable<Sv48> =
        FirstStagePageTable::new(root_page.into()).expect("creating sv48");

    let gpa_base = PageAddr::new(RawAddr::supervisor_virt(0x8000_0000)).unwrap();
    let pte_fields = PteFieldBits::leaf_with_perms(PteLeafPerms::RWX);
    let mapper = page_table
        .map_range(
            gpa_base,
            PageSize::Size4k,
            12, /* TODO: FIXME GIANLUCA */
            &mut || pte_pages.next(),
        )
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
    let elf = ElfLoader::new(bytes).unwrap(); // TODO

    println!("{:?}", elf);

    for h in elf.program_header_iter() {
        // TODO
        println!("{:x?}", h);
    }
    Some(Task::new(page_table))
}
