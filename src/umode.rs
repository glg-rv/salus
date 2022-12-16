// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use crate::umode_mem::UmodeMemoryRange;

use arrayvec::ArrayVec;
use core::cell::{RefCell, RefMut};
use core::arch::global_asm;
use core::mem::size_of;
use memoffset::offset_of;
use riscv_elf::{ElfMap, ElfSegmentPerms};
use riscv_pages::PageSize;
use riscv_regs::Exception::UserEnvCall;
use riscv_regs::{GeneralPurposeRegisters, GprIndex, Readable, Trap, CSR};
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

impl UmodeCpuArchState {
    fn print(&self) {
        let uregs = &self.umode_regs;
        println!(
            "SEPC: 0x{:016x}, SCAUSE: 0x{:016x}, STVAL: 0x{:016x}",
            uregs.sepc, self.trap_csrs.scause, self.trap_csrs.stval,
        );
        use GprIndex::*;
        println!(
            "RA:  0x{:016x}, GP:  0x{:016x}, TP:  0x{:016x}, S0:  0x{:016x}",
            uregs.gprs.reg(RA),
            uregs.gprs.reg(GP),
            uregs.gprs.reg(TP),
            uregs.gprs.reg(S0)
        );
        println!(
            "S1:  0x{:016x}, A0:  0x{:016x}, A1:  0x{:016x}, A2:  0x{:016x}",
            uregs.gprs.reg(S1),
            uregs.gprs.reg(A0),
            uregs.gprs.reg(A1),
            uregs.gprs.reg(A2)
        );
        println!(
            "A3:  0x{:016x}, A4:  0x{:016x}, A5:  0x{:016x}, A6:  0x{:016x}",
            uregs.gprs.reg(A3),
            uregs.gprs.reg(A4),
            uregs.gprs.reg(A5),
            uregs.gprs.reg(A6)
        );
        println!(
            "A7:  0x{:016x}, S2:  0x{:016x}, S3:  0x{:016x}, S4:  0x{:016x}",
            uregs.gprs.reg(A7),
            uregs.gprs.reg(S2),
            uregs.gprs.reg(S3),
            uregs.gprs.reg(S4)
        );
        println!(
            "S5:  0x{:016x}, S6:  0x{:016x}, S7:  0x{:016x}, S8:  0x{:016x}",
            uregs.gprs.reg(S5),
            uregs.gprs.reg(S6),
            uregs.gprs.reg(S7),
            uregs.gprs.reg(S8)
        );
        println!(
            "S9:  0x{:016x}, S10: 0x{:016x}, S11: 0x{:016x}, T0:  0x{:016x}",
            uregs.gprs.reg(S9),
            uregs.gprs.reg(S10),
            uregs.gprs.reg(S11),
            uregs.gprs.reg(T0)
        );
        println!(
            "T1:  0x{:016x}, T2:  0x{:016x}, T3:  0x{:016x}, T4:  0x{:016x}",
            uregs.gprs.reg(T1),
            uregs.gprs.reg(T2),
            uregs.gprs.reg(T3),
            uregs.gprs.reg(T4)
        );
        println!(
            "T5:  0x{:016x}, T6:  0x{:016x}, SP:  0x{:016x}",
            uregs.gprs.reg(T5),
            uregs.gprs.reg(T6),
            uregs.gprs.reg(SP)
        );
    }
}

extern "C" {
    // umode context switch. Defined in umode.S
    fn _run_umode(g: *mut UmodeCpuArchState);
}

#[allow(dead_code)]
const fn host_gpr_offset(index: GprIndex) -> usize {
    offset_of!(UmodeCpuArchState, host_regs)
        + offset_of!(HostCpuRegs, gprs)
        + (index as usize) * size_of::<u64>()
}

#[allow(dead_code)]
const fn umode_gpr_offset(index: GprIndex) -> usize {
    offset_of!(UmodeCpuArchState, umode_regs)
        + offset_of!(UmodeCpuRegs, gprs)
        + (index as usize) * size_of::<u64>()
}

macro_rules! host_csr_offset {
    ($reg:tt) => {
        offset_of!(UmodeCpuArchState, host_regs) + offset_of!(HostCpuRegs, $reg)
    };
}

macro_rules! umode_csr_offset {
    ($reg:tt) => {
        offset_of!(UmodeCpuArchState, umode_regs) + offset_of!(UmodeCpuRegs, $reg)
    };
}

global_asm!(
    include_str!("umode.S"),
    host_ra = const host_gpr_offset(GprIndex::RA),
    host_gp = const host_gpr_offset(GprIndex::GP),
    host_tp = const host_gpr_offset(GprIndex::TP),
    host_s0 = const host_gpr_offset(GprIndex::S0),
    host_s1 = const host_gpr_offset(GprIndex::S1),
    host_a1 = const host_gpr_offset(GprIndex::A1),
    host_a2 = const host_gpr_offset(GprIndex::A2),
    host_a3 = const host_gpr_offset(GprIndex::A3),
    host_a4 = const host_gpr_offset(GprIndex::A4),
    host_a5 = const host_gpr_offset(GprIndex::A5),
    host_a6 = const host_gpr_offset(GprIndex::A6),
    host_a7 = const host_gpr_offset(GprIndex::A7),
    host_s2 = const host_gpr_offset(GprIndex::S2),
    host_s3 = const host_gpr_offset(GprIndex::S3),
    host_s4 = const host_gpr_offset(GprIndex::S4),
    host_s5 = const host_gpr_offset(GprIndex::S5),
    host_s6 = const host_gpr_offset(GprIndex::S6),
    host_s7 = const host_gpr_offset(GprIndex::S7),
    host_s8 = const host_gpr_offset(GprIndex::S8),
    host_s9 = const host_gpr_offset(GprIndex::S9),
    host_s10 = const host_gpr_offset(GprIndex::S10),
    host_s11 = const host_gpr_offset(GprIndex::S11),
    host_sp = const host_gpr_offset(GprIndex::SP),
    host_stvec = const host_csr_offset!(stvec),
    host_sscratch = const host_csr_offset!(sscratch),
    umode_ra = const umode_gpr_offset(GprIndex::RA),
    umode_gp = const umode_gpr_offset(GprIndex::GP),
    umode_tp = const umode_gpr_offset(GprIndex::TP),
    umode_s0 = const umode_gpr_offset(GprIndex::S0),
    umode_s1 = const umode_gpr_offset(GprIndex::S1),
    umode_a0 = const umode_gpr_offset(GprIndex::A0),
    umode_a1 = const umode_gpr_offset(GprIndex::A1),
    umode_a2 = const umode_gpr_offset(GprIndex::A2),
    umode_a3 = const umode_gpr_offset(GprIndex::A3),
    umode_a4 = const umode_gpr_offset(GprIndex::A4),
    umode_a5 = const umode_gpr_offset(GprIndex::A5),
    umode_a6 = const umode_gpr_offset(GprIndex::A6),
    umode_a7 = const umode_gpr_offset(GprIndex::A7),
    umode_s2 = const umode_gpr_offset(GprIndex::S2),
    umode_s3 = const umode_gpr_offset(GprIndex::S3),
    umode_s4 = const umode_gpr_offset(GprIndex::S4),
    umode_s5 = const umode_gpr_offset(GprIndex::S5),
    umode_s6 = const umode_gpr_offset(GprIndex::S6),
    umode_s7 = const umode_gpr_offset(GprIndex::S7),
    umode_s8 = const umode_gpr_offset(GprIndex::S8),
    umode_s9 = const umode_gpr_offset(GprIndex::S9),
    umode_s10 = const umode_gpr_offset(GprIndex::S10),
    umode_s11 = const umode_gpr_offset(GprIndex::S11),
    umode_t0 = const umode_gpr_offset(GprIndex::T0),
    umode_t1 = const umode_gpr_offset(GprIndex::T1),
    umode_t2 = const umode_gpr_offset(GprIndex::T2),
    umode_t3 = const umode_gpr_offset(GprIndex::T3),
    umode_t4 = const umode_gpr_offset(GprIndex::T4),
    umode_t5 = const umode_gpr_offset(GprIndex::T5),
    umode_t6 = const umode_gpr_offset(GprIndex::T6),
    umode_sp = const umode_gpr_offset(GprIndex::SP),
    umode_sepc = const umode_csr_offset!(sepc),
);

/// Errors returned by U-mode runs.
#[derive(Debug)]
pub enum Error {
    /// ELF segment out of range,
    InvalidElf,
    /// Received an unexpected trap while running Umode.
    UnexpectedTrap,
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
        self.run_to_exit();
        match Trap::from_scause(self.arch.trap_csrs.scause).unwrap() {
            Trap::Exception(UserEnvCall) => {
                // Exit on ecall.
                println!("U-mode ecall!");
                self.arch.print();
                Ok(())
            }
            _ => {
                self.arch.print();
                Err(Error::UnexpectedTrap)
            }
        }
    }

    /// Run until it exits
    fn run_to_exit(&mut self) {
        unsafe {
            // Safe to run umode code as it only touches memory assigned to it through umode mappings.
            _run_umode(&mut *self.arch as *mut UmodeCpuArchState);
        }
        // Save off the trap information.
        self.arch.trap_csrs.scause = CSR.scause.get();
        self.arch.trap_csrs.stval = CSR.stval.get();
    }
}
