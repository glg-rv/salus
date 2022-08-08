// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use spin::Once;
use tock_registers::interfaces::Readable;

use super::error::{Error, Result};
use super::registers::*;
use crate::pci::{DeviceId, PciArenaId, PcieRoot, VendorId};

/// IOMMU device. Responsible for managing address translation for PCI devices.
pub struct Iommu {
    _arena_id: PciArenaId,
    registers: &'static mut IommuRegisters,
}

// The global IOMMU singleton.
static IOMMU: Once<Iommu> = Once::new();

// Identifiers from the QEMU RFC implementation.
const IOMMU_VENDOR_ID: u16 = 0x1efd;
const IOMMU_DEVICE_ID: u16 = 0x8001;

impl Iommu {
    /// Probes for the IOMMU device on the given PCI root.
    pub fn probe_from(pci: &PcieRoot) -> Result<()> {
        let arena_id = pci
            .take_and_enable_hypervisor_device(
                VendorId::new(IOMMU_VENDOR_ID),
                DeviceId::new(IOMMU_DEVICE_ID),
            )
            .map_err(Error::ProbingIommu)?;
        let dev = pci.get_device(arena_id).unwrap().lock();

        // IOMMU registers are in BAR0.
        let bar = dev.bar_info().get(0).ok_or(Error::MissingRegisters)?;
        // Unwrap ok: we've already determined BAR0 is valid.
        let pci_addr = dev.get_bar_addr(0).unwrap();
        let regs_base = pci.pci_to_physical_addr(pci_addr).unwrap();
        let regs_size = bar.size();
        if regs_size < core::mem::size_of::<IommuRegisters>() as u64 {
            return Err(Error::InvalidRegisterSize(regs_size));
        }
        if regs_base.bits() % core::mem::size_of::<IommuRegisters>() as u64 != 0 {
            return Err(Error::MisalignedRegisters);
        }
        // Safety: We've taken unique ownership of the IOMMU PCI device and have verified that
        // BAR0 points to a suitably sized and aligned register set.
        let registers = unsafe { (regs_base.bits() as *mut IommuRegisters).as_mut().unwrap() };

        // We need support for Sv48x4 G-stage translation and MSI page-tables at minimum.
        if !registers.capabilities.is_set(Capabilities::Sv48x4) {
            return Err(Error::MissingGStageSupport);
        }
        if !registers.capabilities.is_set(Capabilities::MsiFlat) {
            return Err(Error::MissingMsiSupport);
        }

        let iommu = Iommu {
            _arena_id: arena_id,
            registers,
        };
        IOMMU.call_once(|| iommu);
        Ok(())
    }

    /// Gets a reference to the `Iommu` singleton. Panics if `Iommu::probe_from()` has not yet
    /// been called to initialize it.
    pub fn get() -> &'static Self {
        IOMMU.get().unwrap()
    }

    /// Returns the version of this IOMMU device.
    pub fn version(&self) -> u64 {
        self.registers.capabilities.read(Capabilities::Version)
    }
}

// `Iommu` holds `UnsafeCell`s for register access. Access to these registers is guarded by the
// `Iommu` interface which allow them to be shared and sent between threads.
unsafe impl Send for Iommu {}
unsafe impl Sync for Iommu {}
