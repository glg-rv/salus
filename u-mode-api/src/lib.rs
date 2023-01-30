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
//! There are two says to pass data between the two components:
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
    /// Optional start of mapped area accessible as read-only. Specifies call-specific input.
    pub in_addr: Option<u64>,
    /// If in_addr is valid, length of the area accessible as read-only.
    pub in_len: usize,
    /// Optional start of mapped area accessible as read-write. Specifies call-specific output.
    pub out_addr: Option<u64>,
    /// If in_addr is valid, length of the area accessible as read-write.
    pub out_len: usize,
    /// Optional start of mapped area accessible as read-only. Specifies hypervisor state data.
    pub state_addr: Option<u64>,
    /// if state_addr is valid, length of the area accessible as read-only.
    pub state_len: usize,
}

impl UmodeRequest {
    /// A Nop request: do nothing.
    pub fn nop() -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::Nop,
            in_addr: None,
            in_len: 0,
            out_addr: None,
            out_len: 0,
            state_addr: None,
            state_len: 0,
        }
    }

    /// Print String
    pub fn print_string(addr: u64, len: u64) -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::PrintString,
            in_addr: Some(addr),
            in_len: len as usize,
            out_addr: None,
            out_len: 0,
            state_addr: None,
            state_len: 0,
        }
    }

    /// Copy memory from input to output.
    ///
    /// Caller must guarantee that:
    /// 1. `in_addr` must be mapped user readable for `len` bytes.
    /// 2. `out_addr` must be mapped user writable for `len` bytes.
    pub fn memcopy(out_addr: u64, in_addr: u64, len: u64) -> Option<UmodeRequest> {
        // Check that input and output ranges do not overlap.
        let overlap = core::cmp::max(out_addr, in_addr)
            <= core::cmp::min(out_addr + len - 1, in_addr + len - 1);
        if overlap {
            None
        } else {
            Some(UmodeRequest {
                op: UmodeOp::MemCopy,
                in_addr: Some(in_addr),
                in_len: len as usize,
                out_addr: Some(out_addr),
                out_len: len as usize,
                state_addr: None,
                state_len: 0,
            })
        }
    }

    /// Get Evidence.
    ///
    /// Create a signed certificate from the CSR and the DICE layer state.
    pub fn get_evidence(
        csr_addr: u64,
        csr_len: u64,
        certout_addr: u64,
        certout_len: u64,
        layer_state_addr: u64,
        layer_state_len: u64,
    ) -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::GetEvidence,
            in_addr: Some(csr_addr),
            in_len: csr_len as usize,
            out_addr: Some(certout_addr),
            out_len: certout_len as usize,
            state_addr: Some(layer_state_addr),
            state_len: layer_state_len as usize,
        }
    }
}

impl TryIntoRegisters for UmodeRequest {
    fn try_from_registers(regs: &[u64]) -> Result<UmodeRequest, Error> {
        let req = UmodeRequest {
            op: UmodeOp::try_from(regs[0])?,
            in_addr: if regs[1] == 0 { None } else { Some(regs[1]) },
            in_len: regs[2] as usize,
            out_addr: if regs[3] == 0 { None } else { Some(regs[3]) },
            out_len: regs[4] as usize,
            state_addr: if regs[5] == 0 { None } else { Some(regs[5]) },
            state_len: regs[6] as usize,
        };
        Ok(req)
    }

    fn to_registers(&self, regs: &mut [u64]) {
        regs[0] = self.op as u64;
        regs[1] = if let Some(val) = self.in_addr { val } else { 0 };
        regs[2] = self.in_len as u64;
        regs[3] = if let Some(val) = self.out_addr {
            val
        } else {
            0
        };
        regs[4] = self.out_len as u64;
        regs[5] = if let Some(val) = self.state_addr {
            val
        } else {
            0
        };
        regs[6] = self.state_len as u64;
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
