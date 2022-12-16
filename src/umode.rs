// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use crate::umode_mem::UmodeMemoryRange;

use arrayvec::ArrayVec;
use core::cell::{RefCell, RefMut};
use riscv_elf::{ElfMap, ElfSegmentPerms};
use riscv_pages::PageSize;
use riscv_regs::GeneralPurposeRegisters;
use s_mode_utils::print::*;

/// Host GPR and which must be saved/restored when entering/exiting U-mode.
#[derive(Default)]
#[repr(C)]
struct HostCpuRegs {
    gprs: GeneralPurposeRegisters,
    stvec: u64,
    sscratch: u64,
}

/// Umode GPR and CSR state which must be saved/restored when exiting/entering U-mode.
#[derive(Default)]
#[repr(C)]
struct UmodeCpuRegs {
    gprs: GeneralPurposeRegisters,
    sepc: u64,
}

/// CSRs written on an exit from virtualization that are used by the host to determine the cause of
/// the trap.
#[derive(Default)]
#[repr(C)]
struct TrapRegs {
    scause: u64,
    stval: u64,
}

/// CPU register state that must be saved or restored when entering/exiting U-mode.
#[derive(Default)]
#[repr(C)]
struct UmodeCpuArchState {
    host_regs: HostCpuRegs,
    umode_regs: UmodeCpuRegs,
    trap_csrs: TrapRegs,
}

/// Errors returned by U-mode runs.
#[derive(Debug)]
pub enum Error {
    /// ELF segment out of range,
    InvalidElf,
    /// MAX_RESET_ARGS is too smal for ELF file.
    ResetVectorFull,
}

#[derive(Debug)]
// Holds information on how to resets an U-mode memory area to its original state.
struct UmodeResetArea {
    // The range of memory to be reset.
    range: UmodeMemoryRange,
    // Data to be copied at the beginning of the area.
    data: Option<&'static [u8]>,
}

impl UmodeResetArea {
    fn reset(&self) {
        let range = &self.range;
        let mut copied = 0;
        // Copy data at the beginning.
        if let Some(data) = self.data {
            self.range.copy_to_umode(data);
            copied = data.len();
        }
        // If there is still space available after the copy, zero the rest.
        if copied < range.len() {
            // Unwrap okay. Offset is valid because `copied` is smaller than the range length.
            range.offset(copied).unwrap().clear();
        }
    }
}

// Maximum number of reset areas.
const MAX_RESET_AREAS: usize = 2;

/// Represents the U-mode memory mapping information.
pub struct Umode {
    entry: u64,
    reset_areas: ArrayVec<UmodeResetArea, MAX_RESET_AREAS>,
}

impl Umode {
    /// Initialize U-mode.
    pub fn init(umode_elf: ElfMap<'static>) -> Result<Umode, Error> {
        // Fetch memory regions to reset from the ELF map (all writable regions).
        let mut reset_areas = ArrayVec::new();
        for s in umode_elf
            .segments()
            .filter(|s| s.perms() == &ElfSegmentPerms::ReadWrite)
        {
            // We have to reset the full pages mapped for this segment, not only up to the segment size.
            let mapped_size = PageSize::Size4k.round_up(s.size() as u64);
            let range = UmodeMemoryRange::new(s.vaddr(), mapped_size as usize)
                .map_err(|_| Error::InvalidElf)?;
            let data = if let Some(data) = s.data() {
                // In case data is bigger than segment size, do not write over size. Anything mapped after
                // size should be zero.
                let len = core::cmp::min(s.size(), data.len());
                Some(&data[0..len])
            } else {
                None
            };
            let reset_area = UmodeResetArea { range, data };
            reset_areas
                .try_push(reset_area)
                .map_err(|_| Error::ResetVectorFull)?;
        }
        let entry = umode_elf.entry();
        println!("U-mode entry at {:016x}", entry);
        Ok(Umode { entry, reset_areas })
    }

    /// Create a new U-mode task information. One per each physical CPU.
    pub fn cpu_task(&self) -> UmodeTask {
        let mut arch = UmodeCpuArchState::default();
        arch.umode_regs.sepc = self.entry;
        UmodeTask {
            umode: self,
            arch: RefCell::new(arch),
        }
    }
}

/// Represents a U-mode mappings with its running context.
pub struct UmodeTask<'umode> {
    umode: &'umode Umode,
    arch: RefCell<UmodeCpuArchState>,
}

impl<'umode> UmodeTask<'umode> {
    /// Activate this umode in order to run it.
    /// Return `None` if the task is already active in this CPU.
    pub fn activate(&self) -> Option<UmodeActiveTask> {
        Some(UmodeActiveTask {
            umode: self.umode,
            arch: self.arch.try_borrow_mut().ok()?,
        })
    }
}

/// Represents a U-mode that is running or runnable. Not at initial state.
pub struct UmodeActiveTask<'umode, 'act> {
    umode: &'umode Umode,
    arch: RefMut<'act, UmodeCpuArchState>,
}

impl<'act, 'umode> Drop for UmodeActiveTask<'act, 'umode> {
    // On drop, the umode state gets reset to initial state.
    fn drop(&mut self) {
        self.reset();
    }
}

impl<'act, 'umode> UmodeActiveTask<'act, 'umode> {
    /// Reset U-mode to its initial state.
    pub fn reset(&mut self) {
        // Reset memory to original state.
        for r in &self.umode.reset_areas {
            r.reset();
        }
        // Reset entry.
        *self.arch = UmodeCpuArchState::default();
        self.arch.umode_regs.sepc = self.umode.entry;
    }

    /// Run `umode` until completion or error.
    pub fn run(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
