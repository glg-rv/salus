// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![no_main]

//! # Salus U-mode binary.
//!
//! This is Salus U-mode code. It is used to offload functionalities
//! of the hypervisor in user mode. There's a copy of this task in
//! each CPU.
//!
//! The task it's based on a request loop. This task can be reset at
//! any time by the hypervisor, so it shouldn't hold non-recoverable
//! state.

extern crate libuser;

use data_model::{VolatileMemory, VolatileSlice};
use libuser::*;
use u_mode_api::{Error as UmodeApiError, UmodeRequest};

mod cert;

// Dummy global allocator - panic if anything tries to do an allocation.
struct GeneralGlobalAlloc;

unsafe impl core::alloc::GlobalAlloc for GeneralGlobalAlloc {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        panic!("alloc called!");
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        panic!("dealloc called!");
    }
}

// Global allocator linking (not usage) required by rice dependencies.
#[global_allocator]
static GENERAL_ALLOCATOR: GeneralGlobalAlloc = GeneralGlobalAlloc;

struct UmodeTask {
    vslice: VolatileSlice<'static>,
}

impl UmodeTask {
    // (Test) Print String from U-mode Shared Region
    //
    // U-mode Shared Region:
    //    Contains the data to be printed at the beginning of the area.
    fn op_print_string(&self, len: usize) -> Result<u64, UmodeApiError> {
        // Print maximum 80 chars.
        const MAX_LENGTH: usize = 80;
        let vs_input = self
            .vslice
            .get_slice(0, len)
            .map_err(|_| UmodeApiError::InvalidArgument)?;
        // Copy input from volatile slice.
        let mut input = [0u8; MAX_LENGTH];
        vs_input.copy_to(&mut input[..]);
        let len = core::cmp::min(len, MAX_LENGTH);
        println!(
            "Received a {} bytes string: \"{}\"",
            len,
            core::str::from_utf8(&input[0..len]).map_err(|_| UmodeApiError::InvalidArgument)?
        );
        Ok(len as u64)
    }

    // Copy memory from input to output.
    //
    // Arguments:
    //    out_addr: starting address of output
    //    in_addr: starting address of input
    //    len: length of input and output
    //
    // U-mode Shared Region: Not used.
    fn op_memcopy(&self, out_addr: u64, in_addr: u64, len: usize) -> Result<u64, UmodeApiError> {
        // Safety: we trust the hypervisor to have mapped at `in_addr` `len` bytes for reading.
        let input = unsafe { &*core::ptr::slice_from_raw_parts(in_addr as *const u8, len) };
        // Safety: we trust the hypervisor to have mapped at `out_addr` `len` bytes valid
        // for reading and writing.
        let output = unsafe { &mut *core::ptr::slice_from_raw_parts_mut(out_addr as *mut u8, len) };
        output[0..len].copy_from_slice(&input[0..len]);
        Ok(len as u64)
    }

    // TODO
    //
    // Arguments:
    //   csr_addr: starting address of the input Certificate Signing Request.
    //   csr_len: size of the input Certificate Signing Request.
    //   certout_addr: starting address of the output Certificate.
    //   certout_len: size for the output Certificate.
    //
    // U-mode Shared Region: contains an instance of `GetEvidenceShared`.
    fn op_get_evidence(
        &self,
        csr_addr: u64,
        csr_len: usize,
        certout_addr: u64,
        certout_len: usize,
    ) -> Result<u64, UmodeApiError> {
        // Safety: we trust the hypervisor to have mapped at `csr_addr` `csr_len` bytes for reading.
        let csr = unsafe { &*core::ptr::slice_from_raw_parts(csr_addr as *const u8, csr_len) };
        // Safety: we trust the hypervisor to have mapped at `certout_addr` `certout_len` bytes valid
        // for reading and writing.
        let certout = unsafe {
            &mut *core::ptr::slice_from_raw_parts_mut(certout_addr as *mut u8, certout_len)
        };
        let shared_data = self
            .vslice
            .get_ref(0)
            .map_err(|_| UmodeApiError::Failed)?
            .load();
        cert::get_certificate_sha384(csr, shared_data, certout).map_err(|e| {
            println!("get_certificate failed: {:?}", e);
            use cert::Error::*;
            match e {
                CsrBufferTooSmall(_, _)
                | CsrParseFailed(_)
                | CsrVerificationFailed(_)
                | CertificateBufferTooSmall(_, _) => UmodeApiError::InvalidArgument,
                _ => UmodeApiError::Failed,
            }
        })
    }

    // Run the main loop, receiving requests from the hypervisor and executing them.
    fn run_loop(&self) -> ! {
        let mut res = Ok(0);
        loop {
            // Return result and wait for next operation.
            let req = hyp_nextop(res);
            res = match req {
                Ok(req) => match req {
                    UmodeRequest::Nop => Ok(0),
                    UmodeRequest::MemCopy {
                        out_addr,
                        in_addr,
                        len,
                    } => self.op_memcopy(out_addr, in_addr, len as usize),
                    UmodeRequest::PrintString { len } => self.op_print_string(len),
                    UmodeRequest::GetEvidence {
                        csr_addr,
                        csr_len,
                        certout_addr,
                        certout_len,
                    } => self.op_get_evidence(csr_addr, csr_len, certout_addr, certout_len),
                },
                Err(err) => Err(err),
            };
        }
    }
}

#[no_mangle]
extern "C" fn task_main(cpuid: u64, shared_addr: u64, shared_size: u64) -> ! {
    // Safety: we trust the hypervisor to have mapped an area of memory starting at `shared_addr`
    // valid for at least `shared_size` bytes.
    let vslice =
        unsafe { VolatileSlice::from_raw_parts(shared_addr as *mut u8, shared_size as usize) };
    let task = UmodeTask { vslice };
    println!(
        "umode/#{}: U-mode Shared Region: {:016x} - {} bytes",
        cpuid, shared_addr, shared_size
    );
    task.run_loop()
}
