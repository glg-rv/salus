// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

use crate::{ecall_send, rivos_test, Result, RivosTestFunction, SbiMessage};

/// Copies `len` bytes from `from` to `to`.
/// # Safety
/// Reads from `from` and write to `to`, ensure that's safe.
pub unsafe fn test_memcpy(to: *mut u8, from: *const u8, len: u64) -> Result<()> {
    let msg = SbiMessage::RivosTest(RivosTestFunction::MemCopy(rivos_test::MemCopyArgs {
        to: to as u64,
        from: from as u64,
        len,
    }));
    ecall_send(&msg)?;
    Ok(())
}
