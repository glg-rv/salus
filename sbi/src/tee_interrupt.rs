// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

//! The TEE-AIA extension supplements the TEE extension with hardware-assisted interrupt
//! virtualization using the RISC-V Advanced Interrupt Architecture (AIA) on platforms which
//! support it.

use crate::error::*;
use crate::function::*;

/// Describes a TVM's AIA configuration.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TvmAiaParams {
    /// The base address of the virtualized IMSIC in guest physical address space.
    ///
    /// IMSIC addresses follow the below pattern:
    ///
    /// XLEN-1           >=24                                 12    0
    /// |                  |                                  |     |
    /// -------------------------------------------------------------
    /// |xxxxxx|Group Index|xxxxxxxxxxx|Hart Index|Guest Index|  0  |
    /// -------------------------------------------------------------
    ///
    /// The base address is the address of the IMSIC with group ID, hart ID, and guest ID of 0.
    pub imsic_base_addr: u64,
    /// The number of group index bits in an IMSIC address.
    pub group_index_bits: u32,
    /// The location of the group index in an IMSIC address. Must be >= 24.
    pub group_index_shift: u32,
    /// The number of hart index bits in an IMSIC address.
    pub hart_index_bits: u32,
    /// The number of guest index bits in an IMSIC address. Must be >= log2(guests_per_hart + 1).
    pub guest_index_bits: u32,
    /// The number of guest interrupt files to be implemented per vCPU. Implementations may
    /// reject configurations with guests_per_hart > 0 if nested IMSIC virtualization is not
    /// supported.
    pub guests_per_hart: u32,
}

/// Functions provided by the TEE Interrupt extension.
#[derive(Copy, Clone, Debug)]
pub enum TeeInterruptFunction {
    /// Configures AIA virtualization for the TVM identified by `tvm_id` from the parameters in
    /// the `TvmAiaParams` structure at the non-confidential physical address `params_addr`.
    ///
    /// May only be called prior to TVM finalization.
    ///
    /// Returns 0 on success.
    ///
    /// a6 = 0
    TvmAiaInit {
        /// a0 = TVM ID
        tvm_id: u64,
        /// a1 = physical address of the `TvmAiaParams` structure
        params_addr: u64,
        /// a2 = length of the `TvmAiaParams` structure in bytes
        len: u64,
    },
    /// Sets the guest physical address of the specified vCPU's virtualized IMSIC to `imsic_addr`.
    /// `imsic_addr` must be valid for the AIA configuration that was set in `TvmAiaInit` and no
    /// two vCPUs may share the same `imsic_addr`.
    ///
    /// May only be called prior to TVM finalization and after `TvmAiaInit`. All vCPUs in
    /// an AIA-enabled TVM must their IMSIC configuration set with `TvmCpuSetImsicAddr` prior
    /// to TVM finalization.
    ///
    /// Returns 0 on success.
    ///
    /// a6 - 1
    TvmCpuSetImsicAddr {
        /// a0 = TVM ID
        tvm_id: u64,
        /// a1 = vCPU ID
        vcpu_id: u64,
        /// a2 = guest physical address of vCPU's IMSIC
        imsic_addr: u64,
    },
    /// Converts the non-confidential guest interrupt file at `imsic_addr` for use with a TVM. A TSM
    /// fence sequence must be completed before the interrupt file may be assigned to a child TVM.
    ///
    /// Returns 0 on success.
    ///
    /// a6 = 2
    TsmConvertImsic {
        /// a0 = physical address of interrupt file to be converted
        imsic_addr: u64,
    },
    /// Reclaims the confidential guest interrupt file at `imsic_addr`. The interrupt file must not
    /// currently be assigned to a TVM.
    ///
    /// Returns 0 on success.
    ///
    /// a6 = 3
    TsmReclaimImsic {
        /// a0 = physical address of interrupt file to be reclaimed
        imsic_addr: u64,
    },
}

impl TeeInterruptFunction {
    /// Attempts to parse `Self` from the register values passed in `a0-a7`.
    pub(crate) fn from_regs(args: &[u64]) -> Result<Self> {
        use TeeInterruptFunction::*;
        match args[6] {
            0 => Ok(TvmAiaInit {
                tvm_id: args[0],
                params_addr: args[1],
                len: args[2],
            }),
            1 => Ok(TvmCpuSetImsicAddr {
                tvm_id: args[0],
                vcpu_id: args[1],
                imsic_addr: args[2],
            }),
            2 => Ok(TsmConvertImsic {
                imsic_addr: args[0],
            }),
            3 => Ok(TsmReclaimImsic {
                imsic_addr: args[0],
            }),
            _ => Err(Error::NotSupported),
        }
    }
}

impl SbiFunction for TeeInterruptFunction {
    fn a6(&self) -> u64 {
        use TeeInterruptFunction::*;
        match self {
            TvmAiaInit { .. } => 0,
            TvmCpuSetImsicAddr { .. } => 1,
            TsmConvertImsic { .. } => 2,
            TsmReclaimImsic { .. } => 3,
        }
    }

    fn a0(&self) -> u64 {
        use TeeInterruptFunction::*;
        match self {
            TvmAiaInit {
                tvm_id,
                params_addr: _,
                len: _,
            } => *tvm_id,
            TvmCpuSetImsicAddr {
                tvm_id,
                vcpu_id: _,
                imsic_addr: _,
            } => *tvm_id,
            TsmConvertImsic { imsic_addr } => *imsic_addr,
            TsmReclaimImsic { imsic_addr } => *imsic_addr,
        }
    }

    fn a1(&self) -> u64 {
        use TeeInterruptFunction::*;
        match self {
            TvmAiaInit {
                tvm_id: _,
                params_addr,
                len: _,
            } => *params_addr,
            TvmCpuSetImsicAddr {
                tvm_id: _,
                vcpu_id,
                imsic_addr: _,
            } => *vcpu_id,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        use TeeInterruptFunction::*;
        match self {
            TvmAiaInit {
                tvm_id: _,
                params_addr: _,
                len,
            } => *len,
            TvmCpuSetImsicAddr {
                tvm_id: _,
                vcpu_id: _,
                imsic_addr,
            } => *imsic_addr,
            _ => 0,
        }
    }

    fn a3(&self) -> u64 {
        0
    }

    fn a4(&self) -> u64 {
        0
    }

    fn a5(&self) -> u64 {
        0
    }
}
