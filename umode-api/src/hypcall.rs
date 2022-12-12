// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use crate::Error;

/// Calls from umode to the hypervisors.
pub enum HypCall {
    Base(BaseFunction),
    Demo(DemoFunction),
}

// Hypercall base calls for umode runtime (always implemented).
const HCEXT_BASE: u64 = 0;
// Note: Insert other extensions per functionality here.
const HCEXT_DEMO: u64 = 255;

impl HypCall {
    // Called from hypervisor
    fn from_regs(args: &[u64; 7]) -> Result<HypFunction, Error> {
        match args[7] {
            HCEXT_BASE => BaseFunction::from_regs(args[0..6]).map(HypCall::Base),
            HCEXT_DEMO => DemoFunction::from_regs(args[0..6]).map(HypCall::Base),
            _ => Err(Error::UknownExtension),
        }
    }

    // Called from umode
    fn to_regs(&self) -> [u64; 7] {
        let args = [0u64; 7];
        match *self {
            Base(function) => {
                a[7] = HCEXT_BASE;
                function.to_regs(&args[0..6]);
            }
            Demo(function) => {
                a[7] = HCEXT_DEMO;
                function.to_regs(&args[0..6]);
            }
        }
    }
}

/// The error type returned by calls to hypervisor.
#[repr(u64)]
pub enum HypCallError {
    /// Generic failure in execution of HypCall.
    Failed = 1,
    /// HypCall not supported by hypervisor.
    NotSupported = 2,
}

impl HypCallError {
    pub fn from_code(e: u64) -> Self {
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
    code: u64;
    value: u64;
}

impl HypReturn {
    fn from_regs(ret_args: &[u64; 2]) -> Self {
        Self {
            error_code = ret_args[0];
            return_value = ret_args[1];
        }
    }

    fn to_regs(&self) -> [u64; 2] {
        let mut ret_args = [u64; 2];
        ret_args[0] = self.code;
        ret_args[1] = self.value;
    }
}

impl From<Result<u64>> for HypReturn {
    fn from(result: Result<u64>) -> HypReturn {
        match result {
            Ok(rv) => {
                Self {
                    error_code: HYPC_SUCCESS;
                    error_value: rv,
                }
            }
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

impl From<HypReturn> for Result<u64> {
    fn from(hyp_ret: HypReturn) -> Result<u64) {
        match hyp_ret.error_code {
            HYPC_SUCCESS => Ok(hyp_ret.return_value),
            e => Err(HypCallError::from_code(e)),
        }
    }
}
