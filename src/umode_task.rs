// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use elf_rs::Elf;

use riscv_page_tables::{FirstStagePageTable, PteFieldBits, PteLeafPerms, Sv48};
use riscv_pages::{InternalClean, PageAddr, PageSize, PhysPage, RawAddr, SequentialPages};

use crate::smp::PerCpu;
use crate::task::Task;

/// Loads the task
pub fn load() -> Option<Task> {
    // TODO initialize tasks in main with allocator instead, then store in PerCpu.
    let (user_start, num_user_pages) = PerCpu::this_cpu().user_mode_range();
    // Safety: page is uniquely owned after being taken from those reserved for this purpose.
    // TODO can `user_mode_range` return pages?
    let mut u_pages = unsafe {
        SequentialPages::<InternalClean>::from_mem_range(
            user_start,
            PageSize::Size4k,
            num_user_pages,
        )
        .unwrap()
    };
    let ram_pages = u_pages.take_pages(12).unwrap();
    let mut pte_pages = u_pages.into_iter();
    let root_page = pte_pages.next().unwrap();
    let page_table: FirstStagePageTable<Sv48> =
        FirstStagePageTable::new(root_page.into()).expect("creating sv48");

    let gpa_base = PageAddr::new(RawAddr::supervisor_virt(0x8000_0000)).unwrap();
    let pte_fields = PteFieldBits::leaf_with_perms(PteLeafPerms::RWX);
    let mapper = page_table
        .map_range(gpa_base, PageSize::Size4k, 2, &mut || pte_pages.next())
        .unwrap();
    for (page, gpa) in ram_pages.into_iter().zip(gpa_base.iter_from()) {
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
