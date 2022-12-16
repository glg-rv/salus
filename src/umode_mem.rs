// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

/// U-mode mappings start here.
pub const UMODE_VA_START: u64 = 0xffffffff00000000;
/// Size in bytes of the U-mode VA area.
pub const UMODE_VA_SIZE: u64 = 128 * 1024 * 1024;
/// U-mode mappings end here.
pub const UMODE_VA_END: u64 = UMODE_VA_START + UMODE_VA_SIZE;

/// Returns true if `addr` is contained in the U-mode VA area.
pub fn is_umode_addr(addr: u64) -> bool {
    (UMODE_VA_START..UMODE_VA_END).contains(&addr)
}

/// Returns true if (`addr`, `addr` + `len`) is a valid non-empty range in the VA area.
pub fn is_valid_umode_range(addr: u64, len: usize) -> bool {
    len != 0 && is_umode_addr(addr) && is_umode_addr(addr + len as u64 - 1)
}
