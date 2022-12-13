// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#![no_std]

//! Salus HS <-> HU API.

/// HU -> HS API.
pub mod hypcall;

/// Error for API functions.
pub enum Error {
    /// The API extension called is not supported.
    UnknownExtension,
    /// Request/HyperCall Not Supported.
    NotSupported,
}
