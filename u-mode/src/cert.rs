// Copyright (c) 2023 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

extern crate libuser;
use libuser::*;

use der::Decode;
use generic_array::GenericArray;
use rice::x509::extensions::dice::tcbinfo::DiceTcbInfo;
use rice::x509::certificate::{MAX_CERT_SIZE, Certificate};
use rice::x509::request::CertReq;
use u_mode_api::{attestation::*, Error as UmodeApiError};

pub fn get_certificate_sha384(csr_bytes: &[u8], certout: &mut [u8], data: GetSha384Certificate) -> [u8; MAX_CERT_SIZE] {
    let mut tcb_info_bytes = [0u8; 4096];
    let mut tcb_info = DiceTcbInfo::new();
    let hash_algorithm = const_oid::db::rfc5912::ID_SHA_384;

    let csr = CertReq::from_der(csr_bytes).unwrap(); // TODO REMOVE UNWRAP
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
    let cert_der = Certificate::from_raw_data(data.cdi_id, &data.cdi_id, csr.info.subject.clone(),
            csr.info.public_key, Some(&extensions), &mut cert_der_bytes).unwrap(); // TODO: REMOVE UNWRAP

    println!("cert_der: {:x?}", cert_der);
    cert_der_bytes
}
