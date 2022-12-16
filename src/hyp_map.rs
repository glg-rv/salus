// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use crate::umode_mem::is_valid_umode_range;

use arrayvec::ArrayVec;
use page_tracking::{HwMemMap, HwMemRegion, HwMemRegionType, HwReservedMemType, HypPageAlloc};
use riscv_elf::{ElfMap, ElfSegment, ElfSegmentPerms};
use riscv_page_tables::{FirstStagePageTable, PteFieldBits, PteLeafPerms, Sv48};
use riscv_pages::{
    InternalClean, Page, PageAddr, PageSize, RawAddr, SupervisorPhys, SupervisorVirt,
};
use riscv_regs::{satp, LocalRegisterCopy, SatpHelpers};

// Maximum number of regions unique to every pagetable.
const MAX_PER_PAGETABLE_REGIONS: usize = 32;
// Maximum number of regions shared across all pagetables.
const MAX_SHARED_REGIONS: usize = 32;

// Per-pagetable regions vector.
type PerPagetableRegionsVec = ArrayVec<PerPagetableRegion, MAX_PER_PAGETABLE_REGIONS>;
// Shared regions vector.
type SharedRegionsVec = ArrayVec<SharedRegion, MAX_SHARED_REGIONS>;

#[derive(Debug)]
pub enum Error {
    /// U-mode ELF segment is not page aligned.
    ElfUnalignedSegment,
    /// U-mode ELF segment is not in U-mode VA area.
    ElfInvalidAddress,
}

/// Represents a virtual address region of the hypervisor that will be the same in all pagetables.
pub struct SharedRegion {
    vaddr: PageAddr<SupervisorVirt>,
    paddr: PageAddr<SupervisorPhys>,
    page_count: usize,
    pte_fields: PteFieldBits,
}

impl SharedRegion {
    // Create a shared region from a Hw Memory Map entry.
    fn from_hw_mem_region(r: &HwMemRegion) -> Option<Self> {
        let perms = match r.region_type() {
            HwMemRegionType::Available => {
                // map available memory as rw - unsure what it'll be used for.
                Some(PteLeafPerms::RW)
            }
            HwMemRegionType::Reserved(HwReservedMemType::FirmwareReserved) => {
                // No need to map regions reserved for firmware use
                None
            }
            HwMemRegionType::Reserved(HwReservedMemType::HypervisorImage) => {
                Some(PteLeafPerms::RWX)
            }
            HwMemRegionType::Reserved(HwReservedMemType::HostKernelImage)
            | HwMemRegionType::Reserved(HwReservedMemType::HostInitramfsImage) => {
                Some(PteLeafPerms::R)
            }
            HwMemRegionType::Reserved(HwReservedMemType::HypervisorHeap)
            | HwMemRegionType::Reserved(HwReservedMemType::HypervisorPerCpu)
            | HwMemRegionType::Reserved(HwReservedMemType::PageMap)
            | HwMemRegionType::Mmio(_) => Some(PteLeafPerms::RW),
        };

        if let Some(pte_perms) = perms {
            let paddr = r.base();
            // vaddr == paddr in mapping HW memory map.
            // Unwrap okay. `r.base()` is a page addr so it is aligned to the page.
            let vaddr = PageAddr::new(RawAddr::supervisor_virt(r.base().bits())).unwrap();
            let page_count = PageSize::num_4k_pages(r.size()) as usize;
            let pte_fields = PteFieldBits::leaf_with_perms(pte_perms);
            Some(Self {
                vaddr,
                paddr,
                page_count,
                pte_fields,
            })
        } else {
            None
        }
    }

    // Map this region into a page table.
    fn map(
        &self,
        sv48: &FirstStagePageTable<Sv48>,
        get_pte_page: &mut dyn FnMut() -> Option<Page<InternalClean>>,
    ) {
        let mapper = sv48
            .map_range(
                self.vaddr,
                PageSize::Size4k,
                self.page_count as u64,
                get_pte_page,
            )
            .unwrap();
        for (virt, phys) in self
            .vaddr
            .iter_from()
            .zip(self.paddr.iter_from())
            .take(self.page_count)
        {
            // Safety: all shared regions come from the HW memory map. we will create exactly one
            // mapping for each page and will switch to using that mapping exclusively.
            unsafe {
                mapper.map_addr(virt, phys, self.pte_fields).unwrap();
            }
        }
    }
}

/// Represents a virtual address region that will point to different physical page on each pagetable.
struct PerPagetableRegion {
    // The address space where this region starts.
    vaddr: PageAddr<SupervisorVirt>,
    // Number of bytes of the VA area
    size: usize,
    // PTE bits for the mappings.
    pte_fields: PteFieldBits,
    // Data to be populated at the beginning of the VA area
    data: Option<&'static [u8]>,
}

impl PerPagetableRegion {
    // Creates a per-pagetable region from an U-mode ELF segment.
    fn from_umode_elf_segment(seg: &ElfSegment<'static>) -> Result<Self, Error> {
        // Sanity check for segment alignments.
        //
        // In general ELF might have segments overlapping in the same page, possibly with different
        // permissions. In order to maintain separation and expected permissions on every page, the
        // linker script for umode ELF creates different segments at different pages. Failure to do so
        // would make `map_range()` in `map()` fail.
        //
        // The following check enforces that the segment starts at a 4k page aligned address. Unless
        // the linking is completely corrupt, this also means that it starts at a different page.
        if !PageSize::Size4k.is_aligned(seg.vaddr()) {
            return Err(Error::ElfUnalignedSegment);
        }
        // Sanity check for VA area of the segment.
        if !is_valid_umode_range(seg.vaddr(), seg.size()) {
            return Err(Error::ElfInvalidAddress);
        }
        let pte_perms = match seg.perms() {
            ElfSegmentPerms::ReadOnly => PteLeafPerms::UR,
            ElfSegmentPerms::ReadWrite => PteLeafPerms::URW,
            ElfSegmentPerms::ReadOnlyExecute => PteLeafPerms::URX,
        };
        // Unwrap okay. `seg.vaddr()` has been checked to be 4k aligned.
        let vaddr = PageAddr::new(RawAddr::supervisor_virt(seg.vaddr())).unwrap();
        let pte_fields = PteFieldBits::leaf_with_perms(pte_perms);
        Ok(Self {
            vaddr,
            size: seg.size(),
            pte_fields,
            data: seg.data(),
        })
    }

    // Map this region into a page table.
    fn map(&self, sv48: &FirstStagePageTable<Sv48>, hyp_mem: &mut HypPageAlloc) {
        // Allocate and populate first.
        let page_count = PageSize::num_4k_pages(self.size as u64);
        let pages = hyp_mem.take_pages_for_hyp_state(page_count as usize);
        // Copy data if present.
        if let Some(data) = self.data {
            let dest = pages.base().bits() as *mut u8;
            let len = core::cmp::min(data.len(), self.size);
            // Safe because we copy the minimum between the data size and the VA size.
            unsafe {
                core::ptr::copy(data.as_ptr(), dest, len);
            }
        }
        // Map the populated pages in the page table.
        let mapper = sv48
            .map_range(self.vaddr, PageSize::Size4k, page_count, &mut || {
                hyp_mem.take_pages_for_hyp_state(1).into_iter().next()
            })
            .unwrap();
        for (virt, phys) in self
            .vaddr
            .iter_from()
            .zip(pages.base().iter_from())
            .take(page_count as usize)
        {
            // Safety: all per-pagetable regions are user mappings. User mappings are not considered
            // aliases because they cannot be accessed by supervisor mode directly (sstatus.SUM needs
            // to be 1).
            unsafe {
                mapper.map_addr(virt, phys, self.pte_fields).unwrap();
            }
        }
    }
}

/// A page table that contains hypervisor mappings.
pub struct HypPageTable {
    inner: FirstStagePageTable<Sv48>,
}

impl HypPageTable {
    /// Return the value of the SATP register for this page table.
    pub fn satp(&self) -> u64 {
        let mut satp = LocalRegisterCopy::<u64, satp::Register>::new(0);
        satp.set_from(&self.inner, 0);
        satp.get()
    }
}

/// A set of global mappings of the hypervisor that can be used to create page tables.
pub struct HypMap {
    shared_regions: SharedRegionsVec,
    per_pagetable_regions: PerPagetableRegionsVec,
}

impl HypMap {
    /// Create a new hypervisor map from a hardware memory mem map and a umode ELF.
    pub fn new(mem_map: HwMemMap, umode_elf: &ElfMap<'static>) -> Result<HypMap, Error> {
        // All shared mappings come from the HW Memory Map.
        let shared_regions = mem_map
            .regions()
            .filter_map(SharedRegion::from_hw_mem_region)
            .collect();
        // All per-pagetable mappings come from the U-mode ELF.
        let per_pagetable_regions = umode_elf
            .segments()
            .map(PerPagetableRegion::from_umode_elf_segment)
            .collect::<Result<_, _>>()?;

        Ok(HypMap {
            shared_regions,
            per_pagetable_regions,
        })
    }

    /// Create a new page table based on this memory map.
    pub fn new_page_table(&self, hyp_mem: &mut HypPageAlloc) -> HypPageTable {
        // Create empty sv48 page table
        // Unwrap okay: we expect to have at least one page free.
        let root_page = hyp_mem
            .take_pages_for_hyp_state(1)
            .into_iter()
            .next()
            .unwrap();
        let sv48: FirstStagePageTable<Sv48> =
            FirstStagePageTable::new(root_page).expect("creating first sv48");
        // Map shared regions
        for r in &self.shared_regions {
            r.map(&sv48, &mut || {
                hyp_mem.take_pages_for_hyp_state(1).into_iter().next()
            });
        }
        // Map per-pagetable regions.
        for r in &self.per_pagetable_regions {
            r.map(&sv48, hyp_mem);
        }
        HypPageTable { inner: sv48 }
    }
}
