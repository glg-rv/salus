// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

use core::arch::asm;
use core::cell::{RefCell, RefMut};
use drivers::{imsic::Imsic, CpuId, CpuInfo};
use page_tracking::HypPageAlloc;
use riscv_pages::{
    InternalDirty, PageAddr, PageSize, RawAddr, SequentialPages, SupervisorPageAddr,
};
use riscv_regs::{sstatus, ReadWriteable, CSR};
use s_mode_utils::print::*;
use sbi_rs::api::state;
use sync::Once;

use crate::hyp_map::{HypMap, HypPageTable};
use crate::umode::UmodeTask;
use crate::vm_id::VmIdTracker;

// The secondary CPU entry point, defined in start.S.
extern "C" {
    static _stack_start: u8;
    static _stack_end: u8;
    fn _secondary_start();
}

/// Per-CPU data. A pointer to this struct is loaded into TP when a CPU starts. This structure
/// sits at the top of a secondary CPU's stack.
#[repr(C)]
pub struct PerCpu {
    cpu_id: CpuId,
    vmid_tracker: RefCell<VmIdTracker>,
    page_table: HypPageTable,
    umode_task: Once<RefCell<UmodeTask>>,
    online: Once<bool>,
    stack_top: u64,
}

/// The number of pages we allocate per CPU: the CPU's stack + it's `PerCpu` structure.
const PER_CPU_PAGES: u64 = 0x80;

/// The base address of the per-CPU memory region.
static PER_CPU_BASE: Once<SupervisorPageAddr> = Once::new();

impl PerCpu {
    /// Initializes the `PerCpu` structures for each CPU, taking memory from `mem_map`. This (the
    /// boot CPU's) per-CPU area is initialized and loaded into TP as well.
    pub fn init(boot_hart_id: u64, hyp_mem: &mut HypPageAlloc) {
        let cpu_info = CpuInfo::get();
        let boot_cpu = cpu_info
            .hart_id_to_cpu(boot_hart_id as u32)
            .expect("Cannot find boot CPU ID");

        // Find somewhere to put the per-CPU memory.
        let total_size = PER_CPU_PAGES * cpu_info.num_cpus() as u64 * PageSize::Size4k as u64;
        let pcpu_pages =
            hyp_mem.take_pages_for_hyp_state(PageSize::num_4k_pages(total_size) as usize);
        let pcpu_base = pcpu_pages.base();
        PER_CPU_BASE.call_once(|| pcpu_base);

        VmIdTracker::init();

        // Now initialize each PerCpu structure.
        // Unwrap okay: PER_CPU_PAGES is non zero.
        let mut pcpu_stacks =
            pcpu_pages.into_chunks_iter(core::num::NonZeroU64::new(PER_CPU_PAGES).unwrap());
        for i in 0..cpu_info.num_cpus() {
            let cpu_id = CpuId::new(i);
            // Unwrap okay. We allocated the area `with num_cpus() * PER_CPU_PAGES` pages.
            let pcpu_stack = pcpu_stacks.next().unwrap();
            // Boot CPU is special. Doesn't use the PCPU_BASE area as stack, only for the PCPU area.
            // TODO: Do not allocate PCPU area for boot cpu.
            let stack_pages = if cpu_id == boot_cpu {
                Self::boot_cpu_stack()
            } else {
                // Change state from InternalClean to InternalDirty. Pages are clean but this is not
                // important for the stack (in the boot CPU case, pages are dirty because the stack is
                // in use).
                // Safe because the chunk is composed by hypervisor pages owned by `pcpu_stack`.
                unsafe {
                    SequentialPages::<InternalDirty>::from_mem_range(
                        pcpu_stack.base(),
                        pcpu_stack.page_size(),
                        pcpu_stack.len(),
                    )
                    .unwrap()
                }
            };
            let stack_top = if cpu_id == boot_cpu {
                crate::hyp_layout::HYP_STACKTOP
            } else {
                // Secondary CPUs have PerCpu structure at top of stack.
                crate::hyp_layout::HYP_STACKTOP - core::mem::size_of::<PerCpu>() as u64
            };
            let ptr = Self::ptr_for_cpu(cpu_id);
            let pcpu = PerCpu {
                cpu_id,
                vmid_tracker: RefCell::new(VmIdTracker::new()),
                page_table: HypMap::get().new_page_table(hyp_mem, stack_pages),
                umode_task: Once::new(),
                online: Once::new(),
                stack_top,
            };
            // Safety: ptr is guaranteed to be properly aligned and point to valid memory owned by
            // PerCpu. No other CPUs are alive at this point, so it cannot be concurrently modified
            // either.
            unsafe { core::ptr::write(ptr as *mut PerCpu, pcpu) };
        }

        // Load TP with the address of our PerCpu struct so that we're consistent with secondary
        // CPUs once they're brought up.
        let my_tp = Self::ptr_for_cpu(boot_cpu) as u64;
        unsafe {
            // Safe since we're the only users of TP.
            asm!("mv tp, {rs}", rs = in(reg) my_tp)
        };

        let me = Self::this_cpu();
        me.set_online();
    }

    fn boot_cpu_stack() -> SequentialPages<InternalDirty> {
        // Get the pages of the current stack as created by the linker.
        // Safe because these are linker created variables.
        let stack_start = unsafe { core::ptr::addr_of!(_stack_start) as u64 };
        let stack_end = unsafe { core::ptr::addr_of!(_stack_end) as u64 };
        let stack_startaddr = PageAddr::new(RawAddr::supervisor(stack_start))
            .expect("_stack_start is not page aligned.");
        let stack_endaddr =
            PageAddr::new(RawAddr::supervisor(stack_end)).expect("_stack_end is not page aligned.");
        // Safe because the pages in this range are in the `HypervisorImage` memory region and are only
        // used for the boot cpu stack.
        unsafe {
            SequentialPages::from_page_range(stack_startaddr, stack_endaddr, PageSize::Size4k)
                .unwrap()
        }
    }

    /// Returns a pointer to the `PerCpu` for the given CPU.
    fn ptr_for_cpu(cpu_id: CpuId) -> *const PerCpu {
        let cpu_end = PER_CPU_BASE
            .get()
            .unwrap()
            .checked_add_pages((1 + cpu_id.raw() as u64) * PER_CPU_PAGES)
            .unwrap();
        let pcpu_addr = cpu_end.bits() - core::mem::size_of::<PerCpu>() as u64;
        pcpu_addr as *const PerCpu
    }

    /// Returns this CPU's `PerCpu` structure.
    pub fn this_cpu() -> &'static PerCpu {
        assert!(PER_CPU_BASE.get().is_some()); // Make sure PerCpu has been set up.
        let tp: u64;
        unsafe {
            // Safe since we're the only users of TP.
            asm!("mv {rd}, tp", rd = out(reg) tp)
        };
        let pcpu_ptr = tp as *const PerCpu;
        let pcpu = unsafe {
            // Safe since TP is set up to point to a valid PerCpu struct in init().
            pcpu_ptr.as_ref().unwrap()
        };
        pcpu
    }

    /// Returns this CPU's ID.
    pub fn cpu_id(&self) -> CpuId {
        self.cpu_id
    }

    /// Marks this CPU as online.
    pub fn set_online(&self) {
        self.online.call_once(|| true);
    }

    /// Returns the top of the stack for this CPU.
    pub fn stack_top(&self) -> u64 {
        self.stack_top
    }

    /// Set the CPU umode task (once). Must be called after `PerCpu::init()`.
    pub fn set_umode_task(&self, umode_task: UmodeTask) {
        self.umode_task.call_once(|| RefCell::new(umode_task));
    }

    /// Get the CPU page table. Must be called after `set_cpu_page_table` has been called for this
    /// cpu.
    pub fn page_table(&self) -> &HypPageTable {
        // Unwrap okay: this is called after `set_cpu_page_table`
        &self.page_table
    }

    /// Get the  CPU umode structure. Must be  called after `set_umode_task` has been  called for this
    /// cpu.
    pub fn umode_task_mut(&self) -> RefMut<UmodeTask> {
        // Unwrap okay: this is called after `set_umode_task`
        self.umode_task.get().unwrap().borrow_mut()
    }

    /// Returns a mutable reference to this CPU's VMID tracker.
    pub fn vmid_tracker_mut(&self) -> RefMut<VmIdTracker> {
        self.vmid_tracker.borrow_mut()
    }
}

// PerCpu state obviously cannot be shared between threads.
impl !Sync for PerCpu {}

/// Halts this CPU until an interrupt (for example, delivered via `kick_cpu()`) is received.
pub fn wfi() {
    CSR.sstatus.modify(sstatus::sie.val(1));
    // Safety: WFI behavior is well-defined.
    unsafe { asm!("wfi", options(nomem, nostack)) };
    CSR.sstatus.modify(sstatus::sie.val(0));
}

/// Sends an IPI to `cpu`.
pub fn send_ipi(cpu: CpuId) {
    Imsic::get().send_ipi(cpu).unwrap();
}

/// Boots secondary CPUs, using the HSM SBI call. Upon return, all secondary CPUs will have
/// entered secondary_init().
pub fn start_secondary_cpus() {
    let cpu_info = CpuInfo::get();
    let boot_cpu = PerCpu::this_cpu().cpu_id();
    for i in 0..cpu_info.num_cpus() {
        let cpu_id = CpuId::new(i);
        if cpu_id == boot_cpu {
            continue;
        }

        // Start the hart with it's PerCpu struct in A1; _secondary_start will stash it in TP.
        let pcpu = PerCpu::ptr_for_cpu(cpu_id);
        // Safety: _secondary_start is guaranteed by the linker to be the code to start secondary
        // CPUs. pcpu will only be shared with one cpu.
        unsafe {
            state::hart_start(
                cpu_info.cpu_to_hart_id(cpu_id).unwrap() as u64,
                (_secondary_start as *const fn()) as u64,
                pcpu as u64,
            )
            .expect("Failed to start CPU {i}");
        }

        // Synchronize with the CPU coming online. TODO: Timeout?
        let pcpu = unsafe {
            // Safe since TP is set up to point to a valid PerCpu struct in init().
            pcpu.as_ref().unwrap()
        };
        pcpu.online.wait();
    }

    println!("Brought online {} CPU(s)", cpu_info.num_cpus());
}
