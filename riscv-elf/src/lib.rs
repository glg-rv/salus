#![no_std]

// For testing use the std crate.
#[cfg(test)]
#[macro_use]
extern crate std;

use arrayvec::ArrayVec;
use core::ops::Add;

// Maximum size of Program Headers supported by the loader.
const ELF_PH_MAX: usize = 8;

/// Elf Offset Helper
///
/// An Elf Offset. A separate type to be sure to never used it directly, but only through `slice_*` functions.
#[repr(packed, C)]
#[derive(Copy, Clone, Debug)]
pub struct ElfOffset64 {
    inner: u64,
}

impl ElfOffset64 {
    fn as_usize(&self) -> Option<usize> {
        self.inner.try_into().ok()
    }
}

impl Add<usize> for ElfOffset64 {
    type Output = Self;

    fn add(self, other: usize) -> Self::Output {
        Self {
            inner: self.inner + (other as u64),
        }
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
    if size > 0 {
        slice_check_offset(bytes, offset + (size - 1))
    } else {
        false
    }
}

//fn slice_get_range<'a>(bytes: &'a [u8], offset: ElfOffset64, len: usize) -> Option<&'a [u8]> {
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
    InsufficientBuffer,
    /// The ELF magic number is wrong.
    InvalidMagicNumber,
    /// Unexpected ELF Class
    ELFClass,
    /// Unexted Endiannes
    Endianness,
    /// ELF is not RISC V.
    NotRiscV,
    /// Unexpected ELF version.
    ELFVersion,
    /// Unexpected ELF PH Entry size.
    ELFPhEntrySize,
}

#[derive(Debug)]
pub struct ElfLoader<'elf> {
    file: &'elf [u8],
    ph_array: ArrayVec<&'elf ElfProgramHeader64, ELF_PH_MAX>,
}

impl<'elf> ElfLoader<'elf> {
    pub fn check_offset(&self, offset: ElfOffset64) -> bool {
        slice_check_offset(self.file, offset)
    }

    pub fn check_range(&self, offset: ElfOffset64, size: usize) -> bool {
        slice_check_range(self.file, offset, size)
    }

    pub fn get_range(&self, offset: ElfOffset64, len: usize) -> Option<&'elf [u8]> {
        slice_get_range(self.file, offset, len)
    }

    pub fn new(file: &'elf [u8]) -> Result<ElfLoader<'elf>, Error> {

        // Chek ELF Header

        let hbytes = slice_get_range(file, 0.into(), core::mem::size_of::<ElfHeader64>())
            .ok_or(Error::InsufficientBuffer)?;
        // Safe because we are sure that the size of the slice is the same size as ElfHeader64.
        let header: &'elf ElfHeader64 = unsafe { &*(hbytes.as_ptr() as *const ElfHeader64) };
        // Check magic number
        if header.ei_magic != EI_MAGIC {
            return Err(Error::InvalidMagicNumber);
        }
        // Check is 64bit ELF.
        if header.ei_class != EI_CLASS_64 {
            return Err(Error::ELFClass);
        }
        // Check is Little-Endian
        if header.ei_data != EI_DATA_LE {
            return Err(Error::Endianness);
        }
        // Check ELF versions.
        if header.ei_version != EI_VERSION_1 || header.e_version != E_VERSION_1 {
            return Err(Error::ELFVersion);
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
            return Err(Error::ELFPhEntrySize);
        }
        // Check that we can read the program header table.
        let program_headers = slice_get_range(file, header.e_phoff, phnum * phentsize)
            .ok_or(Error::InsufficientBuffer)?;

        let mut ph_array = ArrayVec::<&ElfProgramHeader64, ELF_PH_MAX>::new();
        for i in 0..phnum {
            let phbytes = slice_get_range(program_headers, (i * phentsize).into(), phentsize)
                .ok_or(Error::InsufficientBuffer)?;
            // Safe because we are sure that the size of the slice is at least as big as ElfProgramHeader64
            let ph: &'elf ElfProgramHeader64 =
                unsafe { &*(phbytes.as_ptr() as *const ElfProgramHeader64) };
            ph_array.push(ph);
        }

        Ok(Self {
            file,
            ph_array,
        })
    }

    pub fn program_header_iter(&self) -> impl Iterator<Item = &&ElfProgramHeader64> {
        self.ph_array.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_test() {
        let file1 = [0u8; 5];
        let file2 = [0u8; 6];
        let off1 = ElfOffset64 { inner: 3 };
        let off2 = ElfOffset64 { inner: 5 };
        let off3 = ElfOffset64 { inner: 6 };

        let r = slice_check_offset(&file1, off1);
        assert_eq!(r, true);
        let r = slice_check_offset(&file1, off2);
        assert_eq!(r, false);
        let r = slice_check_offset(&file1, off3);
        assert_eq!(r, false);

        let r = slice_check_offset(&file2, off1);
        assert_eq!(r, true);
        let r = slice_check_offset(&file2, off2);
        assert_eq!(r, true);
        let r = slice_check_offset(&file2, off3);
        assert_eq!(r, false);

        let r = slice_check_range(&file1, 0.into(), 0);
        assert_eq!(r, false);
        let r = slice_get_range(&file1, 0.into(), 0);
        assert!(r.is_none());

        let r = slice_check_range(&file1, 0.into(), 5);
        assert_eq!(r, true);
        let r = slice_get_range(&file1, 0.into(), 5);
        assert!(r.is_some());
        assert_eq!(r.unwrap().len(), 5);

        let r = slice_check_range(&file1, 0.into(), 6);
        assert_eq!(r, false);
        let r = slice_get_range(&file1, 0.into(), 6);
        assert!(r.is_none());

        let r = slice_check_range(&file1, 4.into(), 1);
        assert_eq!(r, true);
        let r = slice_get_range(&file1, 4.into(), 1);
        assert!(r.is_some());
        assert_eq!(r.unwrap().len(), 1);

        let r = slice_check_range(&file1, 5.into(), 1);
        assert_eq!(r, false);
        let r = slice_get_range(&file1, 5.into(), 1);
        assert!(r.is_none());
    }
}
