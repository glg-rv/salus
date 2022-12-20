// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]

//! Salus Hypervisor <-> U-mode API.
//!
//! This library contains data structures that are passed between
//! hypervisor and umode.

/// The Error type returned returned from this library.
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum Error {
    /// Generic failure in execution.
    Failed = 1,
    /// Ecall not supported. From hypervisor to umode.
    EcallNotSupported = 2,
    /// Request not supported. From umode to hypervisor.
    RequestNotSupported = 3,
}

impl From<u64> for Error {
    fn from(val: u64) -> Error {
        match val {
            1 => Error::Failed,
            2 => Error::EcallNotSupported,
            3 => Error::RequestNotSupported,
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
    fn set_registers(&self, regs: &mut [u64]);
}

/// Trait to transform a type into A-registers when a set of registers might not be able to be
/// transformed into this type, returning an error.
pub trait TryIntoRegisters: Sized {
    /// Get current type from a set of registers or return an error.
    fn try_from_registers(regs: &[u64]) -> Result<Self, Error>;
    /// Write `self` into a set of registers.
    fn set_registers(&self, regs: &mut [u64]);
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

    fn set_registers(&self, regs: &mut [u64]) {
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
    /// Say hello.
    Hello = 2,
}

impl TryFrom<u64> for UmodeOp {
    type Error = Error;

    fn try_from(reg: u64) -> Result<UmodeOp, Error> {
        match reg {
            1 => Ok(UmodeOp::Nop),
            2 => Ok(UmodeOp::Hello),
            _ => Err(Error::RequestNotSupported),
        }
    }
}

/// An operation requested by the hypervisor and executed by umode.
#[derive(Debug)]
pub struct UmodeRequest {
    op: UmodeOp,
    in_addr: Option<u64>,
    in_len: usize,
    out_addr: Option<u64>,
    out_len: usize,
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
        }
    }

    /// Hello World.
    pub fn hello() -> UmodeRequest {
        UmodeRequest {
            op: UmodeOp::Hello,
            in_addr: None,
            in_len: 0,
            out_addr: None,
            out_len: 0,
        }
    }

    /// Returns the requested Operation.
    pub fn op(&self) -> UmodeOp {
        self.op
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
        };
        Ok(req)
    }

    fn set_registers(&self, regs: &mut [u64]) {
        regs[0] = self.op as u64;
        regs[1] = if let Some(val) = self.in_addr { val } else { 0 };
        regs[2] = self.in_len as u64;
        regs[3] = if let Some(val) = self.out_addr {
            val
        } else {
            0
        };
        regs[4] = self.out_len as u64;
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

    fn set_registers(&self, regs: &mut [u64]) {
        match self {
            HypCall::Panic => {
                regs[7] = HYPC_PANIC;
            }
            HypCall::PutChar(byte) => {
                regs[0] = *byte as u64;
                regs[7] = HYPC_PUTCHAR;
            }
            HypCall::NextOp(result) => {
                result.set_registers(regs);
                regs[7] = HYPC_NEXTOP;
            }
        }
    }
}
