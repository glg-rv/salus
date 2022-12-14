// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use arrayvec::ArrayVec;
use page_tracking::{HwMemMap, HwMemRegion, HwMemRegionType, HwReservedMemType, HypPageAlloc};
use riscv_elf::{ElfMap, ElfSegment, ElfSegmentPerms};
use riscv_page_tables::{FirstStagePageTable, PteFieldBits, PteLeafPerms, Sv48};
use riscv_pages::{
    InternalClean, Page, PageAddr, PageSize, RawAddr, SupervisorPhys, SupervisorVirt,
};
use riscv_regs::{satp, LocalRegisterCopy, SatpHelpers};

/// Maximum number of supervisor regions.
const MAX_HYPMAP_SUPERVISOR_REGIONS: usize = 32;
/// Maximum number of U-mode regions.
const MAX_HYPMAP_UMODE_REGIONS: usize = 32;

/// Represents a virtual address region of the hypervisor with a fixed VA->PA Mapping.
pub struct HypMapFixedRegion {
    vaddr: PageAddr<SupervisorVirt>,
    paddr: PageAddr<SupervisorPhys>,
    page_count: usize,
    pte_fields: PteFieldBits,
}

impl HypMapFixedRegion {
    // Create a supervisor VA region from a Hw Memory Map entry.
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
            // Safe as we will create exactly one mapping to each page and will switch to
            // using that mapping exclusively.
            unsafe {
                mapper.map_addr(virt, phys, self.pte_fields).unwrap();
            }
        }
    }
}

// Represents an area that must be reset to restore U-mode original state.
struct UmodeResetArea {
    // The physical address where this region starts.
    paddr: PageAddr<SupervisorPhys>,
    // The size in bytes of this region.
    size: usize,
    // Data to be populated at the beginning of the area.
    data: &'static [u8],
}

impl UmodeResetArea {
    fn reset(&self) {
        let dest = self.paddr.bits() as *mut u8;
        // Clear memory first.
        // Safety: `dest` is `size` bytes in long. The memory is owned by the hypervisor, by
        // construction.
        unsafe {
            core::ptr::write_bytes(dest, 0, self.size);
        }
        // Populate area at the beginning, if data has to be copied.
        let len = core::cmp::min(self.data.len(), self.size);
        // Safety: by construction, `self.pages` in the region are owned by the hypervisor and mapped
        // explicitly to a umode area. Also, we copy the minimum between the data size and the region
        // size, so we'll never read memory outside of `self.data` and write memory outside of the
        // region.
        unsafe {
            core::ptr::copy(self.data.as_ptr(), dest, len);
        }
    }
}

/// Represents a virtual address region used by U-mode. Each page table will have its own copy of the
/// data.
struct HypMapUmodeRegion {
    // The address space where this region starts.
    vaddr: PageAddr<SupervisorVirt>,
    // Number of bytes of the VA area
    size: usize,
    // PTE bits for the mappings.
    pte_fields: PteFieldBits,
    // Data to be populated at the beginning of the VA area
    data: Option<&'static [u8]>,
    // Region is writable and should be reset to the original state.
    resettable: bool,
}

impl HypMapUmodeRegion {
    // Creates an user space virtual address region from a ELF segment.
    fn from_elf_segment(seg: &ElfSegment<'static>) -> Option<Self> {
        // Sanity Check for segment alignments.
        //
        // In general ELF might have segments overlapping in the same page, possibly with different
        // permissions. In order to maintain separation and expected permissions on every page, the
        // linker script for umode ELF creates different segments at different pages. Failure to do so
        // would make `map_range()` in `map()` fail.
        //
        // The following check enforces that the segment starts at a 4k page aligned address. Unless
        // the linking is completely corrupt, this also means that it starts at a different page.
        // Assert is okay. This is a build error.
        assert!(PageSize::Size4k.is_aligned(seg.vaddr()));

        let pte_perms = match seg.perms() {
            ElfSegmentPerms::ReadOnly => PteLeafPerms::UR,
            ElfSegmentPerms::ReadWrite => PteLeafPerms::URW,
            ElfSegmentPerms::ReadOnlyExecute => PteLeafPerms::URX,
        };
        // Unwrap okay. `seg.vaddr()` has been checked to be 4k aligned.
        let vaddr = PageAddr::new(RawAddr::supervisor_virt(seg.vaddr())).unwrap();
        let resettable = pte_perms == PteLeafPerms::URW;
        let pte_fields = PteFieldBits::leaf_with_perms(pte_perms);
        Some(HypMapUmodeRegion {
            vaddr,
            size: seg.size(),
            pte_fields,
            data: seg.data(),
            resettable,
        })
    }

    // Map this region into a page table. Each region is mapped in a contiguous range of the physical
    // address space. This property allows the hypervisor to access this region from the supervisor
    // mapping rather than through user mode mappings.
    fn map(
        &self,
        sv48: &FirstStagePageTable<Sv48>,
        hyp_mem: &mut HypPageAlloc,
    ) -> Option<UmodeResetArea> {
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
            // Safety: it is not an alias because these are user mode mappings and these specific
            // mappings cannot be accessed (without special functions) from supervisor mode.
            unsafe {
                mapper.map_addr(virt, phys, self.pte_fields).unwrap();
            }
        }
        // If writable user region, return a U-mode reset area.
        if self.resettable {
            Some(UmodeResetArea {
                paddr: pages.base(),
                size: self.size,
                data: self.data,
            })
        } else {
            None
        }
    }
}

/// A page table that contains hypervisor mappings.
pub struct HypPageTable {
    inner: FirstStagePageTable<Sv48>,
    umode_reset: ArrayVec<UmodeResetArea, MAX_HYPMAP_UMODE_REGIONS>,
}

impl HypPageTable {
    /// Clear and repopulate the writable areas of U-mode.
    pub fn umode_reset(&self) {
        for reset_area in &self.umode_reset {
            reset_area.reset();
        }
    }

    /// Return the value of the SATP register for this page table.
    pub fn satp(&self) -> u64 {
        let mut satp = LocalRegisterCopy::<u64, satp::Register>::new(0);
        satp.set_from(&self.inner, 0);
        satp.get()
    }
}

/// A set of global mappings of the hypervisor that can be used to create page tables.
pub struct HypMap {
    supervisor_regions: ArrayVec<HypMapFixedRegion, MAX_HYPMAP_SUPERVISOR_REGIONS>,
    umode_regions: ArrayVec<HypMapUmodeRegion, MAX_HYPMAP_UMODE_REGIONS>,
}

impl HypMap {
    /// Create a new hypervisor map from a hardware memory mem map.
    pub fn new(mem_map: HwMemMap, umode_map: &ElfMap<'static>) -> HypMap {
        // All supervisor regions comes from the HW memory map.
        let supervisor_regions = mem_map
            .regions()
            .filter_map(HypMapFixedRegion::from_hw_mem_region)
            .collect();
        // All user regions come from the U-mode map.
        let umode_regions = umode_map
            .segments()
            .filter_map(HypMapUmodeRegion::from_elf_segment)
            .collect();
        HypMap {
            supervisor_regions,
            umode_regions,
        }
    }

    /// Create a new page table based on this memory map.
    pub fn new_page_table(&self, hyp_mem: &mut HypPageAlloc) -> HypPageTable {
        // Create empty sv48 page table
        // Unwrap okay: we expect to have at least one page free or not much will happen anyway.
        let root_page = hyp_mem
            .take_pages_for_hyp_state(1)
            .into_iter()
            .next()
            .unwrap();
        let sv48: FirstStagePageTable<Sv48> =
            FirstStagePageTable::new(root_page).expect("creating first sv48");
        // Map supervisor regions
        for r in &self.supervisor_regions {
            r.map(&sv48, &mut || {
                hyp_mem.take_pages_for_hyp_state(1).into_iter().next()
            });
        }
        // Map umode regions and create umode reset areas.
        let mut umode_reset = ArrayVec::<UmodeResetArea, MAX_HYPMAP_UMODE_REGIONS>::new();
        for r in &self.umode_regions {
            let reset_area = r.map(&sv48, hyp_mem);
            if let Some(area) = reset_area {
                umode_reset.push(area);
            }
        }
        HypPageTable {
            inner: sv48,
            umode_reset,
        }
    }
}
