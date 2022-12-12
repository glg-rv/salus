// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use s_mode_utils::print::*;

use core::arch::global_asm;
use core::mem::size_of;
use memoffset::offset_of;
use riscv_elf::ElfMap;
use riscv_regs::{GeneralPurposeRegisters, GprIndex, Readable, Trap, CSR};

/// Host GPR and which must be saved/restored when entering/exiting a task.
#[derive(Default)]
#[repr(C)]
struct HostCpuRegs {
    gprs: GeneralPurposeRegisters,
    satp: u64,
    stvec: u64,
    sscratch: u64,
}

/// Umode GPR and CSR state which must be saved/restored when exiting/entering a task.
#[derive(Default)]
#[repr(C)]
struct UmodeCpuRegs {
    gprs: GeneralPurposeRegisters,
    satp: u64,
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

/// CPU register state that must be saved or restored when entering/exiting a task.
#[derive(Default)]
#[repr(C)]
struct UmodeCpuState {
    host_regs: HostCpuRegs,
    task_regs: UmodeCpuRegs,
    trap_csrs: TrapRegs,
}

// The task context switch, defined in task.S
extern "C" {
    fn _run_umode(g: *mut UmodeCpuState);
}

#[allow(dead_code)]
const fn host_gpr_offset(index: GprIndex) -> usize {
    offset_of!(UmodeCpuState, host_regs)
        + offset_of!(HostCpuRegs, gprs)
        + (index as usize) * size_of::<u64>()
}

#[allow(dead_code)]
const fn task_gpr_offset(index: GprIndex) -> usize {
    offset_of!(UmodeCpuState, task_regs)
        + offset_of!(UmodeCpuRegs, gprs)
        + (index as usize) * size_of::<u64>()
}

macro_rules! host_csr_offset {
    ($reg:tt) => {
        offset_of!(UmodeCpuState, host_regs) + offset_of!(HostCpuRegs, $reg)
    };
}

macro_rules! task_csr_offset {
    ($reg:tt) => {
        offset_of!(UmodeCpuState, task_regs) + offset_of!(UmodeCpuRegs, $reg)
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
    task_ra = const task_gpr_offset(GprIndex::RA),
    task_gp = const task_gpr_offset(GprIndex::GP),
    task_tp = const task_gpr_offset(GprIndex::TP),
    task_s0 = const task_gpr_offset(GprIndex::S0),
    task_s1 = const task_gpr_offset(GprIndex::S1),
    task_a0 = const task_gpr_offset(GprIndex::A0),
    task_a1 = const task_gpr_offset(GprIndex::A1),
    task_a2 = const task_gpr_offset(GprIndex::A2),
    task_a3 = const task_gpr_offset(GprIndex::A3),
    task_a4 = const task_gpr_offset(GprIndex::A4),
    task_a5 = const task_gpr_offset(GprIndex::A5),
    task_a6 = const task_gpr_offset(GprIndex::A6),
    task_a7 = const task_gpr_offset(GprIndex::A7),
    task_s2 = const task_gpr_offset(GprIndex::S2),
    task_s3 = const task_gpr_offset(GprIndex::S3),
    task_s4 = const task_gpr_offset(GprIndex::S4),
    task_s5 = const task_gpr_offset(GprIndex::S5),
    task_s6 = const task_gpr_offset(GprIndex::S6),
    task_s7 = const task_gpr_offset(GprIndex::S7),
    task_s8 = const task_gpr_offset(GprIndex::S8),
    task_s9 = const task_gpr_offset(GprIndex::S9),
    task_s10 = const task_gpr_offset(GprIndex::S10),
    task_s11 = const task_gpr_offset(GprIndex::S11),
    task_t0 = const task_gpr_offset(GprIndex::T0),
    task_t1 = const task_gpr_offset(GprIndex::T1),
    task_t2 = const task_gpr_offset(GprIndex::T2),
    task_t3 = const task_gpr_offset(GprIndex::T3),
    task_t4 = const task_gpr_offset(GprIndex::T4),
    task_t5 = const task_gpr_offset(GprIndex::T5),
    task_t6 = const task_gpr_offset(GprIndex::T6),
    task_sp = const task_gpr_offset(GprIndex::SP),
    task_sepc = const task_csr_offset!(sepc),
);

/// A structure representing an area to reset after each execution.
struct ResetArea {
    /// Virtual Address start
    vaddr: u64,
    /// Size of the reset area.
    size: usize,
    /// Data that must be copied for reset. In case this is smaller than the area, the rest of the
    /// area will be zeroed.
    data: &'static [u8],
}

/// Salus UMODE task.
pub struct Umode {
    entry: u64,
    cpu_state: UmodeCpuState,
}

impl Umode {
    /// Create a new umode from the ELF map of the user binary.
    pub fn new(umode_map: ElfMap<'static>) -> Self {
        println!("UMODE entry at {:016x}\n", umode_map.entry());
        Umode {
            entry: umode_map.entry(),
            cpu_state: UmodeCpuState::default(),
        }
    }

    /// Run this task until it exits
    fn run_to_exit(&mut self) {
        unsafe {
            // Safe to run umode code as it only touches memory assigned to it through umode mappings.
            _run_umode(&mut self.cpu_state as *mut UmodeCpuState);
        }

        // Save off the trap information.
        self.cpu_state.trap_csrs.scause = CSR.scause.get();
        self.cpu_state.trap_csrs.stval = CSR.stval.get();
    }

    /// Run `umode` until completion or error.
    pub fn run(&mut self) -> Result<(), Error> {
        let regs = &mut self.cpu.state;
        // Setup Entry
        regs.task_regs.sepc = self.entry;
        loop {
            self.run_to_exit();
            match Trap::from_scause(regs.trap_csrs.scause).unwrap() {
                Trap::Exception(UserEnvCall) => {
                    match regs.task_regs.gprs.reg(GprIndex::A0) {
                        0 => {
                            
                        },
                        1 => {
                            return;
                        }
                        2 => {
                            if let Some(c) = char::from_u32(regs.task_regs.reg(GprIndex::A1) as u32) {
                                print!("{}", c);
                            }
                        },
                        _ => {
                            println!("User env call undefined");
                        },
                    }
                }
                _ => {
                    println!("Unexpected Exception");
                },
            }
            self.cpu_state.task_regs.sepc += 4;
        }
    }
}

const UMOP_PANIC: u64 = 0;
const UMOP_RESULT: u64 = 1;
const UMOP_PUTCHAR: u64 = 2;

pub enum UmodeOp {
    /// Panic, i.e. unexpected exit.
    Panic,
    /// Exit with result code.
    Result(u64, u64),
    /// PutChar
    PutChar(u64),
}

impl UmodeOp {
    pub fn from_regs(a: &[u64]) -> Result<Self> {
        match a[7] -> Result<Self> {
            UMOP_PANIC => Ok(UmodeOp::Panic),
            UMOP_RESULT => Ok(UmodeOp::Result(a[0], a[1])),
            UMOP_PUTCHAR => Ok(UmodeOp::PutChar(a[0])),
            _ => Err(Error::NotSupported),
        }
    }
}
