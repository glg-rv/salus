// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]

//! # Salus U-mode API.
//!
//! This library contains data structures that are passed between
//! hypervisor and user mode.
//!
//! All data is passed through `hypcalls`, calls from U-mode to
//! hypervisor, implemented using an `ecall`/`sret` pair.
//!
//! `hypcalls` originate in user mode. They are used to ask the
//! hypervisor for specific services or for signalling end of
//! execution.
//!
//! There are two ways to pass data between the two components:
//! registers and memory.
//!
//! ## Passing Data through Registers.
//!
//! During `ecall` or `sret`, registers `A0`-`A7` (A-registers) are
//! used to pass information between the two components.
//!
//! Details of the specific `hypcall` to run are specified in the
//! A-registers at the moment of the `ecall`. If the `hypcalls`
//! implementation in the hypervisor returns some result then, when
//! `sret` is executed, the A-registers will contain the information
//! returned.
//!
//! This library defines two traits, `IntoRegisters` and
//! `TryIntoRegisters`, that must implemented to allow a type to be
//! passed through registers.
//!
//! ## Passing Data through Memory.
//!
//! TBD.
//!
//! ## Entry of user mode.
//!
//! The user mode process expect to be run from the entry point
//! specified in the ELF file with register `A0` containing a unique
//! u64 ID (the CPU ID).

/// Shared-memory definitions for Attestation functions.
pub mod attestation;

/// The Error type returned returned from this library.
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum Error {
    /// Generic failure in execution.
    Failed = 1,
    /// Invalid arguments passed.
    InvalidArgument = 2,
    /// Ecall not supported. From hypervisor to umode.
    EcallNotSupported = 3,
    /// Request not supported. From umode to hypervisor.
    RequestNotSupported = 4,
}

impl From<u64> for Error {
    fn from(val: u64) -> Error {
        match val {
            1 => Error::Failed,
            2 => Error::InvalidArgument,
            3 => Error::EcallNotSupported,
            4 => Error::RequestNotSupported,
            _ => Error::Failed,
        }
    }
}

// All types that can be passed in registers must implement `IntoRegisters` or `TryIntoRegisters`.

/// Trait to transform a type into A-registers when a set of registers will always transform into
/// this type.
pub trait IntoRegisters {
    /// Get current type from a set of registers.
    fn from_registers(regs: &[u64]) -> Self;
    /// Write `self` into a set of registers.
    fn to_registers(&self, regs: &mut [u64]);
}

/// Trait to transform a type into A-registers when a set of registers might not be able to be
/// transformed into this type, returning an error.
pub trait TryIntoRegisters: Sized {
    /// Get current type from a set of registers or return an error.
    fn try_from_registers(regs: &[u64]) -> Result<Self, Error>;
    /// Write `self` into a set of registers.
    fn to_registers(&self, regs: &mut [u64]);
}

// Result<(), Error> is passed through registers. Implement trait.

// Error code for success.
const HYPC_SUCCESS: u64 = 0;

impl IntoRegisters for Result<(), Error> {
    fn from_registers(regs: &[u64]) -> Result<(), Error> {
        match regs[0] {
            HYPC_SUCCESS => Ok(()),
            e => Err(e.into()),
        }
    }

    fn to_registers(&self, regs: &mut [u64]) {
        match self {
            Ok(_) => {
                regs[0] = HYPC_SUCCESS;
            }
            Err(e) => {
                regs[0] = *e as u64;
            }
        }
    }
}

// UmodeRequest: calls from hypervisor to Umode requesting an operation.

/// Umode operations.
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum UmodeOp {
    /// Do nothing.
    Nop = 1,
    /// (Test) Print data passed in input.
    PrintString = 2,
    /// (Test) Copy memory from input to output.
    MemCopy = 3,
    /// Get attestation evidence from a Certificate Signing Request (CSR)
    GetEvidence = 4,
}

impl TryFrom<u64> for UmodeOp {
    type Error = Error;

    fn try_from(reg: u64) -> Result<UmodeOp, Error> {
        match reg {
            1 => Ok(UmodeOp::Nop),
            2 => Ok(UmodeOp::PrintString),
            3 => Ok(UmodeOp::MemCopy),
            4 => Ok(UmodeOp::GetEvidence),
            _ => Err(Error::RequestNotSupported),
        }
    }
}

/// An operation requested by the hypervisor and executed by umode.
#[derive(Debug)]
pub struct UmodeRequest {
    /// The operation requested.
    pub op: UmodeOp,
    /// Arguments of the operation.
    pub args: [u64; 7],
}

impl UmodeRequest {
    /// A Nop request: do nothing.
    ///
    /// Arguments: none
    /// U-mode Mapped Area: not used.
    pub fn nop() -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::Nop,
            args: [0; 7],
        }
    }

    /// Print String from U-mode Mapped Area
    ///
    /// Arguments:
    ///    [0] = length of data in the U-mode Mapped Area to be printed.
    ///
    /// U-mode Mapped Area:
    ///    Contains the data to be printed at the beginning of the area.
    pub fn print_string(len: usize) -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::PrintString,
            args: [len as u64, 0, 0, 0, 0, 0, 0],
        }
    }

    /// Copy memory from input to output.
    ///
    /// Arguments:
    ///    [0] = starting address of output
    ///    [1] = starting address of input
    ///    [2] = length of input and output
    ///
    /// U-mode Mapped Area: Not used.
    ///
    /// Caller must guarantee that:
    /// 1. `in_addr` must be mapped user readable for `len` bytes.
    /// 2. `out_addr` must be mapped user writable for `len` bytes.
    pub fn memcopy(out_addr: u64, in_addr: u64, len: u64) -> Option<UmodeRequest> {
        // This test call is special because the guest memory in input/output will be used directly
        // by U-mode. Check that input and output ranges do not overlap.
        let overlap = core::cmp::max(out_addr, in_addr)
            <= core::cmp::min(out_addr + len - 1, in_addr + len - 1);
        if overlap {
            None
        } else {
            Some(UmodeRequest {
                op: UmodeOp::MemCopy,
                args: [out_addr, in_addr, len, 0, 0, 0, 0],
            })
        }
    }

    /// Create a signed certificate from the CSR and the DICE layer measurements.
    ///
    /// Arguments:
    ///    [0] = address of the Certificate Signing Request.
    ///    [1] = length of the Certificate Signing Request.
    ///    [2] = address where the output Certificate will be written.
    ///    [3] = length of the area available for the output Certificate .
    ///
    /// U-mode Mapped Area:
    ///    Contains a structure of type `GetSha384Certificate`
    pub fn get_evidence(
        csr_addr: u64,
        csr_len: usize,
        certout_addr: u64,
        certout_len: usize,
    ) -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::GetEvidence,
            args: [
                csr_addr,
                csr_len as u64,
                certout_addr,
                certout_len as u64,
                0,
                0,
                0,
            ],
        }
    }
}

impl TryIntoRegisters for UmodeRequest {
    fn try_from_registers(regs: &[u64]) -> Result<UmodeRequest, Error> {
        let mut args = [0; 7];
        args.as_mut_slice().copy_from_slice(&regs[1..8]);
        let req = UmodeRequest {
            op: UmodeOp::try_from(regs[0])?,
            args,
        };
        Ok(req)
    }

    fn to_registers(&self, regs: &mut [u64]) {
        regs[0] = self.op as u64;
        regs[1..8].copy_from_slice(self.args.as_slice())
    }
}

// HypCall: calls from umode to hypervisor.

/// Calls from umode to the hypervisors.
pub enum HypCall {
    /// Panic and exit immediately.
    Panic,
    /// Print a character for debug.
    PutChar(u8),
    /// Return result of previous request and wait for next operation.
    NextOp(Result<(), Error>),
}

const HYPC_PANIC: u64 = 0;
const HYPC_PUTCHAR: u64 = 1;
const HYPC_NEXTOP: u64 = 2;

impl TryIntoRegisters for HypCall {
    fn try_from_registers(regs: &[u64]) -> Result<Self, Error> {
        match regs[7] {
            HYPC_PANIC => Ok(HypCall::Panic),
            HYPC_PUTCHAR => Ok(HypCall::PutChar(regs[0] as u8)),
            HYPC_NEXTOP => Ok(HypCall::NextOp(Result::from_registers(regs))),
            _ => Err(Error::EcallNotSupported),
        }
    }

    fn to_registers(&self, regs: &mut [u64]) {
        match self {
            HypCall::Panic => {
                regs[7] = HYPC_PANIC;
            }
            HypCall::PutChar(byte) => {
                regs[0] = *byte as u64;
                regs[7] = HYPC_PUTCHAR;
            }
            HypCall::NextOp(result) => {
                result.to_registers(regs);
                regs[7] = HYPC_NEXTOP;
            }
        }
    }
}
