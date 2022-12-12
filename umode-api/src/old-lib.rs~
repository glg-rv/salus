// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]

//! Salus HS <-> HU API.

use core::arch::{asm};


// This library defines the interface between U mode and the hypervisor.
//
// Umode is a single binary. 
// Calls from U mode to Hypervisors.
//
// The API of the calls between user and hypervisor are defined in the `HypCall` structure.  
// At the ABI level this is translated into an `ecall` with parameters passed through the `A`
// registers:
//
// - A7 specifies the hypervisor call.
// - A0-A6 defines the arguments associated with the function (see `HypCallArgs`).



type HypCallArgs = [u8; 6];

#[repr(i64)]
pub enum UmodeError {
    /// Generic failure in execution of the umode call.
    Failed = -1,
    /// Request to umode not supported.
    NotSupported = -2,
}

impl UmodeError {
    pub fn from_code(e: u64) -> Self {

    }
}

pub type UmodeResult = core::result::Result<Option<UserAddressRange>, UmodeError>;

impl UmodeResult {
    pub fn from_args(a: &HypCallArgs) -> Self {
        match e {
            1 => Error::Failed,
            2 => Error::NotSupported,
            _ => Error::Failed,
        }
        
    }

    pub fn to_args(&self) -> HypFunctionArgs {
        
    }
}

type HypCallRegs = [u64; 7];

impl HypCallRegs {
    fn call_id() -> u64 {
        a[0] 
    }

    fn set_call_id(v: u64) {
        a[0] = v;
    }

    fn 
}

// Hypcall Function to register value mappings.
const HFUN_PANIC: u64 = 0;
const HFUN_EXIT: u64 = 1;
const HFUN_PUTCHAR: u64 = 2;

pub enum HypCall {
    /// Panic, i.e. unexpected exit.
    Panic,
    /// Exit with result code.
    Exit(ExitCall);
    /// PutChar
    PutChar(u64),
}

impl HypCall {
    /// Called from hypervisor. Reconstruct HypCall from A registers.
    pub fn from_regs(a: &[u64; 7]) -> Result<Self> {
        let call = a[0];
        let args = a[1..];
        match call {
            HYPC_PANIC => Ok(UmodeEcall::Panic),
            HYPC_RESULT => Ok(UmodeEcall::Exit(UmodeResult::from_args(&args))),
            HYPC_PUTCHAR => Ok(PutChar(a[1] as u8)),
            _ => Err(Error::NotSupported),
        }
    }

    /// Called from umode. Map current HypCall to `ecall` registers.
    pub fn to_regs(&self) -> [u64; 7]; {
        let mut a = [0u64; 3];

        // Parse arguments.
        match *self {
            UmodeEcall::Panic => { a[0] = UMOP_PANIC },
            UmodeEcall::Exit(result) => {
                a[0] = UMOP_EXIT;
                result.to_regs(&mut a[1..7]);
            },
            UmodeEcall::PutChar(c) => {
                a[0] = UMOP_PUTCHAR;
                a[1] = c as u64;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
