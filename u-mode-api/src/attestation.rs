use data_model::DataInit;
use digest::{Digest, OutputSizeUser};

// EvidenceState: data structure for evidence certificate must be passed through memory.

// If this needs be volatile mem, then we need to do things.

/// CDI ID length.
pub const CDI_LEN: usize = 20;
/// Length of a SHA384 hash.
pub const SHA384_LEN: usize = 48;
/// Number of measurement registers.
pub const MSMT_REGISTERS: usize = 8;

/// Compound Device Identifier (CDI) ID type.
pub type CdiId = [u8; CDI_LEN];
/// Measurement registers for the Sha384 case.
pub type MeasurementRegisterSha384 = [u8; SHA384_LEN];

/// State passed to `get_evidence`.
/// Represents the status of the DICE layer needed to generate a
/// certificate.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GetSha384Certificate {
    /// Measurement registers in SHA-384. In `fwid` order.
    pub msmt_regs: [MeasurementRegisterSha384; MSMT_REGISTERS],
    /// CDI Id.
    pub cdi_id: CdiId,
}

// Safety: `LayerStateSha384` is a POD struct without implicit padding and therefore can be
// initialized from a byte array.
unsafe impl DataInit for GetSha384Certificate {}
