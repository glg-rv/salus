// Copyright (c) 2023 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

extern crate libuser;
use libuser::*;

use der::Decode;
use ed25519_dalek::Signer;
use ed25519::Signature;
use generic_array::GenericArray;
use rice::x509::MAX_CSR_LEN;
use rice::x509::certificate::{Certificate, MAX_CERT_SIZE};
use rice::x509::extensions::dice::tcbinfo::DiceTcbInfo;
use rice::x509::request::CertReq;
use u_mode_api::{cert::*, Error as UmodeApiError};

struct UmodeSigner {}

impl Signer<Signature> for UmodeSigner {
    fn try_sign(&self, _: &[u8]) -> Result<Signature, ed25519::Error> {
        Signature::from_bytes(&[0; 64])
    }
}

pub fn get_certificate_sha384(
    csr_input: &[u8],
    data: GetEvidenceShared,
    certout: &mut [u8],
) ->Result<usize, UmodeApiError> {
    // Copy input from U-mode.
    if csr_input.len() > MAX_CSR_LEN {
        //         return Err(EcallError::Sbi(SbiError::InsufficientBufferCapacity));
        panic!("TODO ERROR");
    }
    let mut csr_bytes = [0u8; MAX_CSR_LEN];
    csr_bytes[0..csr_input.len()].copy_from_slice(csr_input);
    
    let mut tcb_info_bytes = [0u8; 4096];
    let mut tcb_info = DiceTcbInfo::new();
    let hash_algorithm = const_oid::db::rfc5912::ID_SHA_384;

    let csr = CertReq::from_der(&csr_bytes[0..csr_input.len()]).unwrap(); // TODO REMOVE UNWRAP
    println!(
        "U-mode CSR version {:?} Signature algorithm {:?}",
        csr.info.version, csr.algorithm.oid
    );

    csr.verify().unwrap(); // TODO: REMOVE UNWRAP

    for m in data.msmt_regs.iter() {
        tcb_info
            .add_fwid::<sha2::Sha384>(hash_algorithm, GenericArray::from_slice(m.as_slice()))
            .unwrap(); // TODO REMOVE UNWRAP
    }

    let tcb_info_extn = tcb_info.to_extension(&mut tcb_info_bytes).unwrap();
    let extensions: [&[u8]; 1] = [tcb_info_extn];

    let mut cert_der_bytes = [0u8; MAX_CERT_SIZE];
    let cert_der = Certificate::from_raw_parts(
        data.cdi_id,
        &data.cdi_id,
        csr.info.subject.clone(),
        csr.info.public_key,
        Some(&extensions),
        &UmodeSigner {},
        &mut cert_der_bytes,
    )
    .unwrap(); // TODO: REMOVE UNWRAP

    if certout.len() < cert_der.len() {
        panic!("TODO");
        //return Err(EcallError::Sbi(SbiError::InvalidParam));
    }
    
    certout[0..cert_der.len()].copy_from_slice(cert_der);
    Ok(cert_der.len())
}
