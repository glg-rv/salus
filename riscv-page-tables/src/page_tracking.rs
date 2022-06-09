// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

// TODO - move to a riscv-specific mutex implementation when ready.
use spin::Mutex;

use alloc::vec::Vec;
use core::alloc::Allocator;
use page_collections::page_box::PageBox;
use page_collections::page_vec::PageVec;
use riscv_pages::{
    MemType, Page, PageOwnerId, PageSize, PhysPage, SequentialPages, SupervisorPageAddr,
};

use crate::page_info::PageMap;
use crate::HwMemMap;

/// Errors related to managing physical page information.
#[derive(Debug)]
pub enum Error {
    /// Too many guests started by the host at once.
    GuestOverflow,
    /// Too many guests per system(u64 overflow).
    IdOverflow,
    /// The given page isn't physically present.
    InvalidPage(SupervisorPageAddr),
    /// The ownership chain is too long to add another owner.
    OwnerOverflow,
    /// The page would become unowned as a result of popping its current owner.
    OwnerUnderflow,
    /// Attempt to pop the owner of an unowned page.
    UnownedPage,
    /// Attempt to modify the owner of a reserved page.
    ReservedPage,
}

pub type Result<T> = core::result::Result<T, Error>;

// Inner struct that is wrapped in a mutex by `PageTracker`.
struct PageTrackerInner {
    next_owner_id: u64,
    active_guests: PageVec<PageOwnerId>,
    pages: PageMap,
}

impl PageTrackerInner {
    // pops any owners that have exited.
    // Remove owners of the page that have since terminated. This is done lazily as needed to
    // prevent a long running operation on guest exit.
    fn pop_exited_owners(&mut self, addr: SupervisorPageAddr) {
        if let Some(info) = self.pages.get_mut(addr) {
            info.pop_owners_while(|id| !self.active_guests.contains(id));
        }
    }

    // Pop the current owner returning the page to the previous owner. Returns the removed owner ID.
    fn pop_owner_internal(&mut self, addr: SupervisorPageAddr) -> Result<PageOwnerId> {
        let page_info = self.pages.get_mut(addr).unwrap();
        page_info.pop_owner()
    }

    // Sets the owner of the page at `addr` to `owner`
    fn set_page_owner(&mut self, addr: SupervisorPageAddr, owner: PageOwnerId) -> Result<()> {
        self.pop_exited_owners(addr);

        let page_info = self.pages.get_mut(addr).ok_or(Error::InvalidPage(addr))?;
        page_info.push_owner(owner)
    }

    // Returns the current owner of the the page ad `addr`.
    fn owner(&self, addr: SupervisorPageAddr) -> Option<PageOwnerId> {
        let info = self.pages.get(addr)?;
        info.find_owner(|id| self.active_guests.contains(id))
    }

    // Returns the type of memory the page represents.
    fn mem_type(&self, addr: SupervisorPageAddr) -> Option<MemType> {
        let info = self.pages.get(addr)?;
        Some(info.mem_type())
    }
}

/// This struct wraps the list of all memory pages and active guests. It can be cloned and passed to
/// other compontents that need access to page state. Once created, there is no way to free the
/// backing page list. That page list is needed for the lifetime of the system.
#[derive(Clone)]
pub struct PageTracker {
    inner: &'static Mutex<PageTrackerInner>,
}

impl PageTracker {
    /// Creates a new PageTracker representing all pages in the system and returns all pages that are
    /// available for the primary host to use, starting at the next `host_alignment`-aligned chunk.
    pub fn from<A: Allocator>(
        mut hyp_mem: HypPageAlloc<A>,
        host_alignment: u64,
    ) -> (Self, Vec<SequentialPages, A>) {
        // TODO - hard coded to two pages worth of guests. - really dumb if page size is 1G
        let mut active_guests = PageVec::from(hyp_mem.take_pages(2));
        active_guests.push(PageOwnerId::host());

        let state_storage_page = hyp_mem.next_page();

        // Discard a host_alignment sized chunk to align ourselves.
        let _ = hyp_mem.take_pages_with_alignment(
            (host_alignment / PageSize::Size4k as u64)
                .try_into()
                .unwrap(),
            host_alignment,
        );

        let (page_map, host_pages) = hyp_mem.drain();

        let mutex_box = PageBox::new_with(
            Mutex::new(PageTrackerInner {
                // Start at two for owners as host and hypervisor reserve 0 and 1.
                next_owner_id: 2,
                active_guests,
                pages: page_map,
            }),
            state_storage_page,
        );

        (
            Self {
                inner: PageBox::leak(mutex_box),
            },
            host_pages,
        )
    }

    /// Adds a new guest to the system, giving it the next ID.
    pub fn add_active_guest(&self) -> Result<PageOwnerId> {
        let mut page_tracker = self.inner.lock();
        // unwrap is fine as next_owner_id is guaranteed to be valid.
        let id = PageOwnerId::new(page_tracker.next_owner_id).unwrap();
        // TODO handle very rare roll over cleaner.
        page_tracker.next_owner_id = page_tracker
            .next_owner_id
            .checked_add(1)
            .ok_or(Error::IdOverflow)?;

        page_tracker
            .active_guests
            .try_reserve(1)
            .map_err(|_| Error::GuestOverflow)?;
        page_tracker.active_guests.push(id);
        Ok(id)
    }

    /// Removes an active guest previously added by `add_active_guest`.
    pub fn rm_active_guest(&self, remove_id: PageOwnerId) {
        let mut page_tracker = self.inner.lock();
        page_tracker.active_guests.retain(|&id| id != remove_id);
    }

    /// Sets the owner of the page at the given `addr` to `owner`.
    pub fn set_page_owner(&self, addr: SupervisorPageAddr, owner: PageOwnerId) -> Result<()> {
        let mut page_tracker = self.inner.lock();
        page_tracker.set_page_owner(addr, owner)
    }

    /// Removes the current owner of the page at `addr` and returns it.
    pub fn pop_owner(&self, addr: SupervisorPageAddr) -> Result<PageOwnerId> {
        let mut page_tracker = self.inner.lock();
        page_tracker.pop_owner_internal(addr)
    }

    /// Returns the current owner of the page.
    pub fn owner(&self, addr: SupervisorPageAddr) -> Option<PageOwnerId> {
        let page_tracker = self.inner.lock();
        page_tracker.owner(addr)
    }

    /// Returns the type of memory the page represents.
    pub fn mem_type(&self, addr: SupervisorPageAddr) -> Option<MemType> {
        let page_tracker = self.inner.lock();
        page_tracker.mem_type(addr)
    }
}

/// `HypPageAlloc` is created from the hardware memory map and builds the array of PageInfo
/// structs for all pages in the system. It is used to allocate pages for the hypervisor at
/// startup for building the host VM and other local data. Once the hypervisor has taken the
/// pages it needs, `HypPageAlloc` should be converted to the list of remaining free memory
/// regions to be mapped into the host with `drain()`.
pub struct HypPageAlloc<A: Allocator> {
    next_page: SupervisorPageAddr,
    pages: PageMap,
    alloc: A,
}

impl<A: Allocator> HypPageAlloc<A> {
    /// Creates a new `HypPageAlloc`. The memory map passed in contains information about what
    /// physical memory can be used by the machine.
    pub fn new(mem_map: HwMemMap, alloc: A) -> Self {
        // Unwrap here (and below) since we can't continue if there isn't any free memory.
        let first_page = mem_map.regions().next().unwrap().base();
        let page_map = PageMap::build_from(mem_map);
        let first_avail_page = page_map
            .iter_from(first_page)
            .unwrap()
            .find(|p| p.page.is_free())
            .unwrap()
            .addr;
        Self {
            next_page: first_avail_page,
            pages: page_map,
            alloc,
        }
    }

    /// Takes ownership of the remaining free pages in the system page map and adds them to 'ranges'.
    /// It also returns the global page info structs as `PageMap`.
    pub fn drain(mut self) -> (PageMap, Vec<SequentialPages, A>) {
        let mut ranges = Vec::new_in(self.alloc);
        while self.pages.get(self.next_page).is_some() {
            // Find the last page in this contiguous range.
            let last_page = self
                .next_page
                .iter_from()
                .find(|&a| match self.pages.get(a) {
                    Some(p) => !p.is_free(),
                    _ => true,
                })
                .unwrap();

            // Now take ownership.
            for page in self.next_page.iter_from().take_while(|&a| a != last_page) {
                self.pages
                    .get_mut(page)
                    .unwrap()
                    .push_owner(PageOwnerId::hypervisor())
                    .unwrap();
            }

            // Safe to create this range as they were previously free and we just took
            // ownership.
            let range = unsafe {
                // Unwrap ok; pages are always 4kB-aligned.
                SequentialPages::from_page_range(self.next_page, last_page, PageSize::Size4k)
                    .unwrap()
            };
            ranges.push(range);

            // Skip until the next free page or we reach the end of memory.
            self.next_page = last_page
                .iter_from()
                .find(|&a| match self.pages.get(a) {
                    Some(p) => p.is_free(),
                    _ => true,
                })
                .unwrap();
        }

        (self.pages, ranges)
    }

    /// Returns the number of pages remaining in the system. Note that this may include reserved
    /// pages.
    pub fn pages_remaining(&self) -> u64 {
        // Ok to unwrap because next page must be in range.
        self.pages.num_after(self.next_page).unwrap() as u64
    }

    /// Returns the next 4k page for the hypervisor to use.
    /// Asserts if out of memory. If there aren't enough pages to set up hypervisor state, there is
    /// no point in continuing.
    pub fn next_page(&mut self) -> Page {
        // OK to unwrap as next_page is guaranteed to be in range.
        self.pages
            .get_mut(self.next_page)
            .unwrap()
            .push_owner(PageOwnerId::hypervisor())
            .expect("Failed to take ownership");

        let page = unsafe {
            // Safe to create a page here since `next_page` was previously free, implying that we
            // had unique ownership of the page and that it is an ordinary RAM page.
            Page::new(self.next_page)
        };
        // unwrap here because if physical memory runs out before setting up basic hypervisor
        // structures, the system can't continue.
        self.next_page = self
            .pages
            .iter_from(self.next_page)
            .unwrap()
            .find(|p| p.page.is_free())
            .unwrap()
            .addr;
        page
    }

    /// Takes `count` contiguous Pages with the requested alignment from the system map. Sets
    /// the hypervisor as the owner of the pages, and any pages consumed up until that point,
    /// in the system page map. Allows passing ranges of pages around without a mutable
    /// reference to the global owners list. Panics if there are not `count` pages available.
    pub fn take_pages_with_alignment(&mut self, count: usize, align: u64) -> SequentialPages {
        // Helper to test whether a contiguous range of `count` pages is free and aligned.
        let range_is_free_and_aligned = |start: SupervisorPageAddr| {
            let end = start.checked_add_pages(count as u64).unwrap();
            if start.bits() & (align - 1) != 0 {
                return false;
            }
            start
                .iter_from()
                .take_while(|&a| a != end)
                .all(|a| self.pages.get(a).map_or(false, |p| p.is_free()))
        };

        // Find the free page rage and mark it, and any free pages we skipped in between,
        // as hypervisor-owned.
        let first_page = self
            .pages
            .iter_from(self.next_page)
            .unwrap()
            .find(|p| range_is_free_and_aligned(p.addr))
            .unwrap()
            .addr;
        let last_page = first_page.checked_add_pages(count as u64).unwrap();
        for page in self.next_page.iter_from().take_while(|&a| a != last_page) {
            if let Some(page_info) = self.pages.get_mut(page) {
                if page_info.is_free() {
                    // OK to unwrap as this struct is new and must have space for one owner.
                    page_info.push_owner(PageOwnerId::hypervisor()).unwrap();
                }
            }
        }

        // Move self's next page past these taken pages.
        self.next_page = self
            .pages
            .iter_from(last_page)
            .unwrap()
            .find(|p| p.page.is_free())
            .unwrap()
            .addr;

        unsafe {
            // It's safe to create a page range of the memory that `self` forfeited ownership of
            // above and the new `SequentialPages` is now the unique owner. Ok to unwrap here simce
            // all pages are trivially aligned to 4kB.
            SequentialPages::from_page_range(first_page, last_page, PageSize::Size4k).unwrap()
        }
    }

    /// Same as above, but without any alignment requirements.
    pub fn take_pages(&mut self, count: usize) -> SequentialPages {
        self.take_pages_with_alignment(count, PageSize::Size4k as u64)
    }
}

impl<A: Allocator> Iterator for HypPageAlloc<A> {
    type Item = Page;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_page())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HwMemMapBuilder;
    use alloc::alloc::Global;
    use riscv_pages::RawAddr;

    fn stub_hyp_mem() -> HypPageAlloc<Global> {
        const ONE_MEG: usize = 1024 * 1024;
        const MEM_ALIGN: usize = 2 * ONE_MEG;
        const MEM_SIZE: usize = 256 * ONE_MEG;
        let backing_mem = vec![0u8; MEM_SIZE + MEM_ALIGN];
        let aligned_pointer = unsafe {
            // Not safe - just a test
            backing_mem
                .as_ptr()
                .add(backing_mem.as_ptr().align_offset(MEM_ALIGN))
        };
        let start_pa = RawAddr::supervisor(aligned_pointer as u64);
        let hw_map = unsafe {
            // Not safe - just a test
            HwMemMapBuilder::new(PageSize::Size4k as u64)
                .add_memory_region(start_pa, MEM_SIZE.try_into().unwrap())
                .unwrap()
                .build()
        };
        let hyp_mem = HypPageAlloc::new(hw_map, Global);
        // Leak the backing ram so it doesn't get freed
        std::mem::forget(backing_mem);
        hyp_mem
    }

    fn stub_page_tracker() -> (PageTracker, Vec<SequentialPages, Global>) {
        let hyp_mem = stub_hyp_mem();
        let (page_tracker, host_mem) = PageTracker::from(hyp_mem, PageSize::Size4k as u64);
        (page_tracker, host_mem)
    }

    #[test]
    fn hyp_mem_take_pages() {
        let mut hyp_mem = stub_hyp_mem();
        let first = hyp_mem.next_page();
        let mut taken = hyp_mem.take_pages(2).into_iter();
        let after_taken = hyp_mem.next_page();

        assert_eq!(
            after_taken.addr().bits(),
            first.addr().bits() + (PageSize::Size4k as u64 * 3)
        );
        assert_eq!(
            taken.next().unwrap().addr().bits(),
            first.addr().bits() + PageSize::Size4k as u64
        );
        assert_eq!(
            taken.next().unwrap().addr().bits(),
            first.addr().bits() + (PageSize::Size4k as u64 * 2)
        );
    }

    #[test]
    fn hyp_mem_take_by_ref() {
        let mut hyp_mem = stub_hyp_mem();
        let first = hyp_mem.next_page();
        let mut taken = hyp_mem.by_ref().take_pages(2).into_iter();
        let after_taken = hyp_mem.next_page();

        assert_eq!(
            after_taken.addr().bits(),
            first.addr().bits() + (PageSize::Size4k as u64 * 3)
        );
        assert_eq!(
            taken.next().unwrap().addr().bits(),
            first.addr().bits() + PageSize::Size4k as u64
        );
        assert_eq!(
            taken.next().unwrap().addr().bits(),
            first.addr().bits() + (PageSize::Size4k as u64 * 2)
        );
    }

    #[test]
    fn hyp_mem_take_aligned() {
        let mut hyp_mem = stub_hyp_mem();
        let range = hyp_mem.take_pages_with_alignment(4, 16 * 1024);
        assert_eq!(range.base().bits() & (16 * 1024 - 1), 0);
    }

    #[test]
    fn hyp_mem_drain() {
        let hyp_mem = stub_hyp_mem();
        let remaining = hyp_mem.pages_remaining();
        let (_, host_pages) = hyp_mem.drain();
        assert_eq!(host_pages.len(), 1);
        assert_eq!(
            host_pages[0].length_bytes(),
            remaining * PageSize::Size4k as u64
        );
    }

    #[test]
    fn drop_one_page_tracker_ref() {
        let (page_tracker, _host_mem) = stub_page_tracker();
        let new_id = {
            let c = page_tracker.clone();
            c.add_active_guest().unwrap()
        };
        assert_eq!(page_tracker.inner.lock().active_guests.len(), 2);

        page_tracker.rm_active_guest(new_id);

        assert_eq!(page_tracker.inner.lock().active_guests.len(), 1);
    }
}
