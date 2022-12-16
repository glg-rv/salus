// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use riscv_regs::{sstatus, ReadWriteable, CSR};
use s_mode_utils::print::*;

/// U-mode mappings start here.
pub const UMODE_VA_START: u64 = 0xffffffff00000000;
/// Size in bytes of the U-mode VA area.
pub const UMODE_VA_SIZE: u64 = 128 * 1024 * 1024;
/// U-mode mappings end here.
pub const UMODE_VA_END: u64 = UMODE_VA_START + UMODE_VA_SIZE;

/// Returns true if `addr` is contained in the U-mode VA area.
pub fn is_umode_addr(addr: u64) -> bool {
    (UMODE_VA_START..UMODE_VA_END).contains(&addr)
}

/// Returns true if (`addr`, `addr` + `len`) is a valid non-empty range in the VA area.
pub fn is_valid_umode_range(addr: u64, len: usize) -> bool {
    len != 0 && is_umode_addr(addr) && is_umode_addr(addr + len as u64 - 1)
}

/// Errors returned by `umode_mem`.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    /// Address is not in the VA area for U-mode.
    InvalidAddress,
    /// Length is invalid.
    InvalidLength,
    /// Offset is invalid.
    InvalidOffset,
}

#[derive(Debug)]
/// A valid range in U-mode VA-area.
pub struct UmodeMemoryRange {
    /// Start of the U-mode memory range.
    addr: u64,
    /// Lenght of the U-mode memory range.
    len: usize,
}

impl UmodeMemoryRange {
    /// Create a new U-mode memory range. Succeeds if this is a valid range in the U-mode VA area.
    pub fn new(addr: u64, len: usize) -> Result<UmodeMemoryRange, Error> {
        if len == 0 {
            Err(Error::InvalidLength)
        } else if !is_valid_umode_range(addr, len) {
            Err(Error::InvalidAddress)
        } else {
            Ok(UmodeMemoryRange { addr, len })
        }
    }

    /// Creates a subrange of current range starting at offset `off`.
    pub fn offset(&self, off: usize) -> Result<UmodeMemoryRange, Error> {
        if off < self.len {
            Ok(UmodeMemoryRange {
                addr: self.addr + off as u64,
                len: self.len - off,
            })
        } else {
            Err(Error::InvalidOffset)
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Copy from hypervisor to the beginning of this memory range.
    /// Caller must ensure that the U-mode memory range is mapped.
    pub fn copy_to_umode(&self, data: &[u8]) {
        let len = core::cmp::min(data.len(), self.len);
        let dest = self.addr as *mut u8;
        println!("Copying from data to {:#?} for {:?} bytes", dest, len);
        // Caller guarantees mapping is present. Write to user mapping setting SUM in SSTATUS.
        CSR.sstatus.modify(sstatus::sum.val(1));
        // Safe because `len` is not bigger than the length of this U-mode range starting at `dest`.
        unsafe {
            core::ptr::copy(data.as_ptr(), dest, len);
        }
        CSR.sstatus.modify(sstatus::sum.val(0));
    }

    /// Zero the memory in this range.
    /// Caller must ensure that the U-mode memory range is mapped.
    pub fn clear(&self) {
        let dest = self.addr as *mut u8;
        println!("Clearing from data to {:#?} for {:?} bytes", dest, self.len);
        // Caller guarantees mapping is present. Write to user mapping setting SUM in SSTATUS.
        CSR.sstatus.modify(sstatus::sum.val(1));
        // Safe because the range starting at `dest` is exactly `self.len` long.
        unsafe {
            core::ptr::write_bytes(dest, 0, self.len);
        }
        CSR.sstatus.modify(sstatus::sum.val(0));
    }
}
