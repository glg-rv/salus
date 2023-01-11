// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use crate::smp::PerCpu;
use crate::vm_pages::PinnedPages;

use arrayvec::ArrayVec;
use core::cell::RefCell;
use page_tracking::{HwMemMap, HwMemRegion, HwMemRegionType, HwReservedMemType, HypPageAlloc};
use riscv_elf::{ElfMap, ElfSegment, ElfSegmentPerms};
use riscv_page_tables::{
    tlb::sfence_vma, FirstStagePageTable, PagingMode, PteFieldBits, PteLeafPerms, Sv48,
};
use riscv_pages::{
    InternalClean, Page, PageAddr, PageSize, RawAddr, SeqPageIter, SupervisorPhys, SupervisorVirt,
};
use riscv_regs::{satp, LocalRegisterCopy, SatpHelpers};

// Maximum number of regions unique to every pagetable (private).
const MAX_PRIVATE_REGIONS: usize = 32;
// Maximum number of regions shared across all pagetables.
const MAX_SHARED_REGIONS: usize = 32;

// Private regions vector.
type PrivateRegionsVec = ArrayVec<PrivateRegion, MAX_PRIVATE_REGIONS>;
// Shared regions vector.
type SharedRegionsVec = ArrayVec<SharedRegion, MAX_SHARED_REGIONS>;

/// Errors returned by creating or modifying hypervisor mappings.
#[derive(Debug)]
pub enum Error {
    /// U-mode ELF segment is not page aligned.
    ElfUnalignedSegment,
    /// U-mode ELF segment is not in U-mode VA area.
    ElfInvalidAddress,
    /// Invalid U-mode Slot Number
    InvalidSlot,
    /// Not enough space on the U-mode map area.
    OutOfMap,
    /// Could not create a mapper for the U-mode area.
    MapperCreationFailed,
    /// Could not map the U-mode area.
    MapFailed,
    /// Could not unmap the U-mode area.
    UnmapFailed,
}

// Represents a virtual address region of the hypervisor that will be the same in all pagetables.
struct SharedRegion {
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

// U-mode binary mappings start here.
const UMODE_VA_START: u64 = 0xffffffff00000000;
// Size in bytes of the U-mode binary VA area.
const UMODE_VA_SIZE: u64 = 128 * 1024 * 1024;
// U-mode binary mappings end here.
const UMODE_VA_END: u64 = UMODE_VA_START + UMODE_VA_SIZE;

// The addresses between `UMODE_MAPPINGS_START` and `UMODE_MAPPINGS_END` is an area of the private
// page table where the hypervisor can map pages shared from guest VMs. The area is divided in
// slots, of equal size `UMODE_MAPPING_SLOT_SIZE`.
const UMODE_MAPPING_SLOT_SIZE: u64 = 4 * 1024 * 1024;
//The number of slots available for mapping.
// mapping.
const UMODE_MAPPING_SLOTS: u64 = 2;

// Start of the private U-mode mappings area.
const UMODE_MAPPINGS_START: u64 = UMODE_VA_END + 4 * 1024 * 1024;
// Maximum size of the private mappings area.
const UMODE_MAPPINGS_SIZE: u64 = UMODE_MAPPING_SLOTS * UMODE_MAPPING_SLOT_SIZE;

// Returns true if `addr` is contained in the U-mode VA area.
fn is_umode_addr(addr: u64) -> bool {
    (UMODE_VA_START..UMODE_VA_END).contains(&addr)
}

// Returns true if (`addr`, `addr` + `len`) is a valid non-empty range in the VA area.
fn is_valid_umode_range(addr: u64, len: usize) -> bool {
    len != 0 && is_umode_addr(addr) && is_umode_addr(addr + len as u64 - 1)
}

// Represents a virtual address region that will point to different physical page on each pagetable.
struct PrivateRegion {
    // The address space where this region starts.
    vaddr: PageAddr<SupervisorVirt>,
    // Number of bytes of the VA area
    size: usize,
    // PTE bits for the mappings.
    pte_fields: PteFieldBits,
    // Data to be populated at the beginning of the VA area
    data: Option<&'static [u8]>,
}

impl PrivateRegion {
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
    /// The pagetable containing hypervisor mappings.
    sv48: FirstStagePageTable<Sv48>,
    /// A pte page pool for U-mode mappings.
    pte_pages: RefCell<SeqPageIter<InternalClean>>,
}

impl HypPageTable {
    /// Return the value of the SATP register for this page table.
    pub fn satp(&self) -> u64 {
        let mut satp = LocalRegisterCopy::<u64, satp::Register>::new(0);
        satp.set_from(&self.sv48, 0);
        satp.get()
    }

    // Return the virtual address of U-mode mapping slot `slot`.
    fn umode_slot_va(slot: u64) -> Result<PageAddr<SupervisorVirt>, Error> {
        if slot < UMODE_MAPPING_SLOTS {
            // Unwrap okay: the result is dependent on constant that must be page aligned.
            Ok(PageAddr::new(RawAddr::supervisor_virt(
                UMODE_MAPPINGS_START + slot * UMODE_MAPPING_SLOT_SIZE,
            ))
            .unwrap())
        } else {
            Err(Error::InvalidSlot)
        }
    }

    fn map_umode_slot(
        &self,
        slot: u64,
        pages: &PinnedPages,
        writable: bool,
    ) -> Result<PageAddr<SupervisorVirt>, Error> {
        let num_pages = pages.range().num_pages();
        if num_pages > PageSize::num_4k_pages(UMODE_MAPPING_SLOT_SIZE) {
            return Err(Error::OutOfMap);
        }
        let vaddr = Self::umode_slot_va(slot)?;
        let mapper = self
            .sv48
            .map_range(vaddr, PageSize::Size4k, num_pages, &mut || {
                self.pte_pages.borrow_mut().next()
            })
            .map_err(|_| Error::MapperCreationFailed)?;
        let perms = if writable {
            PteFieldBits::leaf_with_perms(PteLeafPerms::URW)
        } else {
            PteFieldBits::leaf_with_perms(PteLeafPerms::UR)
        };
        for (virt, phys) in vaddr
            .iter_from()
            .zip(pages.range().base().iter_from())
            .take(num_pages as usize)
        {
            // Safe because pages are mapped in U-mode and cannot be accessed by the hypervisor directly.
            unsafe {
                mapper
                    .map_addr(virt, phys, perms)
                    .map_err(|_| Error::MapFailed)?;
            }
        }
        Ok(vaddr)
    }

    pub fn unmap_umode_slot(&self, slot: u64, num_pages: u64) -> Result<(), Error> {
        let vaddr = Self::umode_slot_va(slot)?;
        if num_pages > PageSize::num_4k_pages(UMODE_MAPPING_SLOT_SIZE) {
            return Err(Error::OutOfMap);
        }
        self.sv48
            .unmap_range(vaddr, PageSize::Size4k, num_pages)
            .map_err(|_| Error::UnmapFailed)?;
        // update TLB at each hypervisor map change.
        sfence_vma(None, None);
        Ok(())
    }
}

/// A set of global mappings of the hypervisor that can be used to create page tables.
pub struct HypMap {
    shared_regions: SharedRegionsVec,
    private_regions: PrivateRegionsVec,
}

impl HypMap {
    // Create a new hypervisor map from a hardware memory mem map and a umode ELF.
    pub fn new(mem_map: HwMemMap, umode_elf: &ElfMap<'static>) -> Result<HypMap, Error> {
        // All shared mappings come from the HW Memory Map.
        let shared_regions = mem_map
            .regions()
            .filter_map(SharedRegion::from_hw_mem_region)
            .collect();
        // All private mappings come from the U-mode ELF.
        let private_regions = umode_elf
            .segments()
            .map(PrivateRegion::from_umode_elf_segment)
            .collect::<Result<_, _>>()?;
        Ok(HypMap {
            shared_regions,
            private_regions,
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
        // Map regions shared across all pagetables.
        for r in &self.shared_regions {
            r.map(&sv48, &mut || {
                hyp_mem.take_pages_for_hyp_state(1).into_iter().next()
            });
        }
        // Map regions unique to a pagetable.
        for r in &self.private_regions {
            r.map(&sv48, hyp_mem);
        }
        // Alloc pte_pages for U-mode mappings.
        let pte_pages = hyp_mem
            .take_pages_for_hyp_state(Sv48::max_pte_pages(
                UMODE_MAPPINGS_SIZE / PageSize::Size4k as u64,
            ) as usize)
            .into_iter();
        HypPageTable {
            sv48,
            pte_pages: RefCell::new(pte_pages),
        }
    }
}

pub struct UmodeMapping {
    slot: u64,
    vaddr: PageAddr<SupervisorVirt>,
    pages: PinnedPages,
}

impl UmodeMapping {
    pub fn vaddr(&self) -> PageAddr<SupervisorVirt> {
        self.vaddr
    }
    pub fn new(slot: u64, pages: PinnedPages, writable: bool) -> Result<Self, Error> {
        let vaddr = PerCpu::this_cpu()
            .page_table()
            .map_umode_slot(slot, &pages, writable)?;
        Ok(UmodeMapping { slot, vaddr, pages })
    }
}

// On drop, UmodeMapping will first unmap the pages, then unpin them (through PinnedPages drop).
impl Drop for UmodeMapping {
    fn drop(&mut self) {
        let page_table = PerCpu::this_cpu().page_table();
        // Unwrap okay: on construction slot was mapped, and the unmap must succeed.
        page_table
            .unmap_umode_slot(self.slot, self.pages.range().num_pages())
            .unwrap();
    }
}
