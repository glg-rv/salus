// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

/// Trait to be defined to create a hypercall extension.
pub trait HypCallExt: Sized {
    /// Transform into registers. Called from umode.
    fn from_regs(args: &[u64]) -> Result<Self, HypCallError>;
    /// Build from registers. Called from hypervisor.
    fn to_regs(&self, args: &mut [u64]);
}

/// Calls from umode to the hypervisors.
pub enum HypCall {
    /// Base API. Needed for execution of umode.
    Base(BaseFunc),
    //    Demo(DemoFunction),
}

// Hypercall base calls for umode runtime (always implemented).
const HCEXT_BASE: u64 = 0;
// Note: Insert other extensions per functionality here.
//const HCEXT_DEMO: u64 = 255;

impl HypCall {
    /// Create an hypercall structure from registers. Called from hypervisor.
    pub fn from_regs(args: &[u64]) -> Result<HypCall, HypCallError> {
        use HypCall::*;
        match args[7] {
            HCEXT_BASE => Ok(Base(BaseFunc::from_regs(&args[0..6])?)),
            //            HCEXT_DEMO => Ok(Demo(DemoFunc::from_regs(&mut args[0..6])?)),
            _ => Err(HypCallError::UnknownExtension)
        }
    }

    /// Translate a `self` to registers. Called from umode.
    pub fn to_regs(&self, args: &mut [u64]) {
        match self {
            HypCall::Base(function) => {
                args[7] = HCEXT_BASE;
                function.to_regs(&mut args[0..6]);
            } /*            HypCall::Demo(function) => {
                              args[7] = HCEXT_DEMO;
                              function.to_regs(&mut args[0..6]);
                          }
              */
        };
    }
}

/// The error type returned by calls to hypervisor.
#[repr(u64)]
pub enum HypCallError {
    /// Generic failure in execution of HypCall.
    Failed = 1,
    /// HypCall not supported by hypervisor.
    NotSupported = 2,
    /// HypCall extension not implemented.
    UnknownExtension,
}

impl HypCallError {
    fn from_code(e: u64) -> Self {
        use HypCallError::*;
        match e {
            1 => Failed,
            2 => NotSupported,
            _ => Failed,
        }
    }
}

// Error code for success.
const HYPC_SUCCESS: u64 = 0;

/// Return type for hypcalls. Sent through A-registers, maps into a Result<u64, HypCallError>.
pub struct HypReturn {
    error_code: u64,
    return_value: u64,
}

impl HypReturn {
    /// Create a `HypReturn` from registers.
    pub fn from_regs(ret_args: &[u64]) -> Self {
        Self {
            error_code: ret_args[0],
            return_value: ret_args[1],
        }
    }

    /// Translate `self` into registers.
    pub fn to_regs(&self, args: &mut [u64]) {
        args[0] = self.error_code;
        args[1] = self.return_value;
    }
}

impl From<Result<u64, HypCallError>> for HypReturn {
    fn from(result: Result<u64, HypCallError>) -> HypReturn {
        match result {
            Ok(rv) => Self {
                error_code: HYPC_SUCCESS,
                return_value: rv,
            },
            Err(e) => Self::from(e),
        }
    }
}

impl From<HypCallError> for HypReturn {
    fn from(error: HypCallError) -> HypReturn {
        HypReturn {
            error_code: error as u64,
            return_value: 0,
        }
    }
}

impl From<HypReturn> for Result<u64, HypCallError> {
    fn from(hyp_ret: HypReturn) -> Result<u64, HypCallError> {
        match hyp_ret.error_code {
            HYPC_SUCCESS => Ok(hyp_ret.return_value),
            e => Err(HypCallError::from_code(e)),
        }
    }
}

/// The base extension of hypcalls. Necessary for basic runtime operations.
pub enum BaseFunc {
    /// Panic and exit immediately.
    Panic,
    /// Print a character for debug.
    PutChar(u8),
}

const HYPC_BASE_PANIC: u64 = 0;
const HYPC_BASE_PUTCHAR: u64 = 1;

impl HypCallExt for BaseFunc {
    fn to_regs(&self, regs: &mut [u64]) {
        match self {
            BaseFunc::Panic => {
                regs[0] = HYPC_BASE_PANIC;
            }
            BaseFunc::PutChar(byte) => {
                regs[0] = HYPC_BASE_PUTCHAR;
                regs[1] = *byte as u64;
            }
        }
    }

    fn from_regs(regs: &[u64]) -> Result<Self, HypCallError> {
        match regs[0] {
            HYPC_BASE_PANIC => Ok(BaseFunc::Panic),
            HYPC_BASE_PUTCHAR => Ok(BaseFunc::PutChar(regs[1] as u8)),
            _ => Err(HypCallError::NotSupported),
        }
    }
}
