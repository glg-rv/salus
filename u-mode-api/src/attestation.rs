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
/// Array of measurement registers. In `fwid` order.
pub type MeasurementRegistersSha384 = [[u8; SHA384_LEN]; MSMT_REGISTERS];

#[repr(C)]
/// State passed to `get_evidence`.
/// Represents the status of the DICE layer needed to generate a
/// certificate.
pub struct LayerStateSha384 {
    /// Status of the measurement registers.
    pub msmt_regs: MeasurementRegistersSha384,
    /// CDI Id.
    pub cdi_id: CdiId,
}
