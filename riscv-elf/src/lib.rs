#![no_std]

//! RiscV ELF loader library for salus

// For testing use the std crate.
#[cfg(test)]
#[macro_use]
extern crate std;

use arrayvec::ArrayVec;

// Maximum size of Program Headers supported by the loader.
const ELF_SEGMENTS_MAX: usize = 8;

/// Elf Offset Helper
///
/// An Elf Offset. A separate type to be sure to never used it
/// directly, but only through `slice_*` functions.
#[repr(packed, C)]
#[derive(Copy, Clone, Debug)]
pub struct ElfOffset64 {
    inner: u64,
}

impl ElfOffset64 {
    fn as_usize(&self) -> Option<usize> {
        self.inner.try_into().ok()
    }

    fn usize_add(self, other: usize) -> Option<ElfOffset64> {
        let inner = self.inner.checked_add(other as u64)?;
        Some(Self { inner })
    }
}

impl From<usize> for ElfOffset64 {
    fn from(val: usize) -> Self {
        Self { inner: val as u64 }
    }
}

fn slice_check_offset(bytes: &[u8], offset: ElfOffset64) -> bool {
    if let Some(offset) = offset.as_usize() {
        bytes.len() > offset
    } else {
        // If offset doesn't fit in a usize, it's definitely out of bound.
        false
    }
}

fn slice_check_range(bytes: &[u8], offset: ElfOffset64, size: usize) -> bool {
    if size < 1 {
        return false;
    }

    if let Some(last) = offset.usize_add(size - 1) {
        slice_check_offset(bytes, last)
    } else {
        false
    }
}

fn slice_get_range(bytes: &[u8], offset: ElfOffset64, len: usize) -> Option<&[u8]> {
    if slice_check_range(bytes, offset, len) {
        // Unwrap safe because check_range succeeded, will fit into `usize`.
        let start = offset.as_usize().unwrap();
        Some(&bytes[start..start + len])
    } else {
        None
    }
}

/// ELF64 Program Header Table Entry
#[repr(packed, C)]
#[derive(Copy, Clone, Debug)]
pub struct ElfProgramHeader64 {
    p_type: u32,
    p_flags: u32,
    p_offset: ElfOffset64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

// ELF Segment Types
/// The array element is unused
pub const PT_NULL: u32 = 0;
/// The array element specifies a loadable segment
pub const PT_LOAD: u32 = 1;

// Elf Segment Permission
/// Execute
pub const PF_X: u32 = 0x1;
/// Write
pub const PF_W: u32 = 0x2;
/// Read
pub const PF_R: u32 = 0x4;

/// ELF64 Header
#[repr(packed, C)]
#[derive(Copy, Clone, Debug)]
pub struct ElfHeader64 {
    ei_magic: [u8; 4],
    ei_class: u8,
    ei_data: u8,
    ei_version: u8,
    ei_osabi: u8,
    ei_abiversion: u8,
    ei_pad: [u8; 7],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: ElfOffset64,
    e_shoff: ElfOffset64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

const EI_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const EI_CLASS_64: u8 = 2;
const EI_DATA_LE: u8 = 1;
const EI_VERSION_1: u8 = 1;
const E_MACHINE_RISCV: u16 = 0xf3;
const E_VERSION_1: u32 = 1;

/// ELF Loader Errors.
#[derive(Debug)]
pub enum Error {
    /// Requested to read after EOF.
    BadOffset,
    /// The ELF magic number is wrong.
    InvalidMagicNumber,
    /// Unexpected ELF Class
    InvalidClass,
    /// Unsupported Endiannes
    InvalidEndianness,
    /// ELF is not RISC V.
    NotRiscV,
    /// Unexpected ELF version.
    BadElfVersion,
    /// Unexpected ELF PH Entry size.
    BadEntrySize,
    /// Malformed Program Header.
    ProgramHeaderMalformed,
    /// Segment Permissions Unsupported
    UnsupportedSegmentFlags(u32),
}

#[derive(Debug)]
pub enum ElfSegmentPerms {
    R,
    RW,
    RX,
}

/// A structure representing a segment.
#[derive(Debug)]
pub struct ElfSegment<'elf> {
    data: &'elf [u8],
    vaddr: u64,
    size: usize,
    perms: ElfSegmentPerms,
}

impl<'elf> ElfSegment<'elf> {
    fn new(
        data: &'elf [u8],
        vaddr: u64,
        size: usize,
        flags: u32,
    ) -> Result<ElfSegment<'elf>, Error> {
        let perms = if flags == PF_R {
            Ok(ElfSegmentPerms::R)
        } else if flags == PF_R | PF_W {
            Ok(ElfSegmentPerms::RW)
        } else if flags == PF_R | PF_X {
            Ok(ElfSegmentPerms::RX)
        } else {
            Err(Error::UnsupportedSegmentFlags(flags))
        }?;
        // Check size is valid
        vaddr
            .checked_add(size as u64)
            .ok_or(Error::ProgramHeaderMalformed)?;
        Ok(ElfSegment {
            data,
            vaddr,
            size,
            perms,
        })
    }

    pub fn data(&self) -> &'elf [u8] {
        self.data
    }

    pub fn vaddr(&self) -> u64 {
        self.vaddr
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn perms(&self) -> &ElfSegmentPerms {
        &self.perms
    }
    /*
        pub fn vaddr(&self) -> SupervisorPageAddr {
            SupervisorPageAddr::new(RawAddr::supervisor(self.vaddr))
        }

        pub fn num_4k_pages(&self) -> u64 {
            let base = PageSize::Size4k.round_down(self.vaddr);
            // Unwrap okay. We checked size at creation.
            let end = self.vaddr.checked_add(self.size as u64).unwrap();
            PageSize::num_4k_pages(end - base)
        }

        pub fn populate(&self, range: SupervisorPageRange) -> Result<()> {
            let offset = self.vaddr - PageSize::Size4k.round_down(self.vaddr);
            let len = core::cmp::min(range.length_bytes(), data.len());
            let dst = (range.base().bits() + offset) as *const u8;
            unsafe {
                core::ptr::copy(self.data, dst, len);
            }
        }

        pub fn flags(&self) {
            self.flags
        }
    */
}

/// A structure that checks and prepares and ELF for loading into memory.
#[derive(Debug)]
pub struct ElfMap<'elf> {
    bytes: &'elf [u8],
    segments: ArrayVec<ElfSegment<'elf>, ELF_SEGMENTS_MAX>,
}

impl<'elf> ElfMap<'elf> {
    /// Create a new ElfMap from a slice containing an ELF file.
    pub fn new(bytes: &'elf [u8]) -> Result<ElfMap<'elf>, Error> {
        // Chek ELF Header

        let hbytes = slice_get_range(bytes, 0.into(), core::mem::size_of::<ElfHeader64>())
            .ok_or(Error::BadOffset)?;
        // Safe because we are sure that the size of the slice is the same size as ElfHeader64.
        let header: &'elf ElfHeader64 = unsafe { &*(hbytes.as_ptr() as *const ElfHeader64) };
        // Check magic number
        if header.ei_magic != EI_MAGIC {
            return Err(Error::InvalidMagicNumber);
        }
        // Check is 64bit ELF.
        if header.ei_class != EI_CLASS_64 {
            return Err(Error::InvalidClass);
        }
        // Check is Little-Endian
        if header.ei_data != EI_DATA_LE {
            return Err(Error::InvalidEndianness);
        }
        // Check ELF versions.
        if header.ei_version != EI_VERSION_1 || header.e_version != E_VERSION_1 {
            return Err(Error::BadElfVersion);
        }
        // Check is RISC-V.
        if header.e_machine != E_MACHINE_RISCV {
            return Err(Error::NotRiscV);
        }

        // Check Program Header Table
        let phnum = header.e_phnum as usize;
        let phentsize = header.e_phentsize as usize;
        // Check that e_phentsize is >= of size of ElfProgramHeader64
        if core::mem::size_of::<ElfProgramHeader64>() > phentsize {
            return Err(Error::BadEntrySize);
        }
        // Check that we can read the program header table.
        let program_headers =
            slice_get_range(bytes, header.e_phoff, phnum * phentsize).ok_or(Error::BadOffset)?;

        // Load segments
        let mut segments = ArrayVec::<ElfSegment, ELF_SEGMENTS_MAX>::new();
        let num_segs = core::cmp::min(phnum, ELF_SEGMENTS_MAX);
        for i in 0..num_segs {
            // Find the i-th ELF Program Header.
            let phbytes = slice_get_range(program_headers, (i * phentsize).into(), phentsize)
                .ok_or(Error::BadOffset)?;
            // Safe because we are sure that the size of the slice is at least as big as ElfProgramHeader64
            let ph: &'elf ElfProgramHeader64 =
                unsafe { &*(phbytes.as_ptr() as *const ElfProgramHeader64) };

            // Ignore if not a load segment.
            if ph.p_type != PT_LOAD {
                continue;
            }

            // Create a segment from the PH.
            let datasz: usize = ph
                .p_filesz
                .try_into()
                .map_err(|_| Error::ProgramHeaderMalformed)?;
            let data = slice_get_range(bytes, ph.p_offset, datasz).ok_or(Error::BadOffset)?;
            let vaddr = ph.p_vaddr;
            let size: usize = ph
                .p_memsz
                .try_into()
                .map_err(|_| Error::ProgramHeaderMalformed)?;
            let flags = ph.p_flags;
            let segment = ElfSegment::new(data, vaddr, size, flags)?;
            segments.push(segment);
        }

        Ok(Self { bytes, segments })
    }

    /// Return an iterator containings loadable segments of this ELF file.
    pub fn segments(&'elf self) -> impl Iterator<Item = &'elf ElfSegment> {
        self.segments.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_test() {
        let bytes1 = [0u8; 5];
        let bytes2 = [0u8; 6];
        let off1 = ElfOffset64 { inner: 3 };
        let off2 = ElfOffset64 { inner: 5 };
        let off3 = ElfOffset64 { inner: 6 };

        let r = slice_check_offset(&bytes1, off1);
        assert_eq!(r, true);
        let r = slice_check_offset(&bytes1, off2);
        assert_eq!(r, false);
        let r = slice_check_offset(&bytes1, off3);
        assert_eq!(r, false);

        let r = slice_check_offset(&bytes2, off1);
        assert_eq!(r, true);
        let r = slice_check_offset(&bytes2, off2);
        assert_eq!(r, true);
        let r = slice_check_offset(&bytes2, off3);
        assert_eq!(r, false);

        let r = slice_check_range(&bytes1, 0.into(), 0);
        assert_eq!(r, false);
        let r = slice_get_range(&bytes1, 0.into(), 0);
        assert!(r.is_none());

        let r = slice_check_range(&bytes1, 0.into(), 5);
        assert_eq!(r, true);
        let r = slice_get_range(&bytes1, 0.into(), 5);
        assert!(r.is_some());
        assert_eq!(r.unwrap().len(), 5);

        let r = slice_check_range(&bytes1, 0.into(), 6);
        assert_eq!(r, false);
        let r = slice_get_range(&bytes1, 0.into(), 6);
        assert!(r.is_none());

        let r = slice_check_range(&bytes1, 4.into(), 1);
        assert_eq!(r, true);
        let r = slice_get_range(&bytes1, 4.into(), 1);
        assert!(r.is_some());
        assert_eq!(r.unwrap().len(), 1);

        let r = slice_check_range(&bytes1, 5.into(), 1);
        assert_eq!(r, false);
        let r = slice_get_range(&bytes1, 5.into(), 1);
        assert!(r.is_none());
    }
}
