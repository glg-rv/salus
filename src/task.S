// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0


/// Enter the task given in `TaskInfo` from `a0`.
.section .text.init
.global _run_task
_run_task:
    /* Save hypervisor state */

    /* Save Host GPRs (except T0-T6 and a0, which is TaskInfo and stashed in sscratch) */
    sd   ra, ({host_ra})(a0)
    sd   gp, ({host_gp})(a0)
    sd   tp, ({host_tp})(a0)
    sd   s0, ({host_s0})(a0)
    sd   s1, ({host_s1})(a0)
    sd   a1, ({host_a1})(a0)
    sd   a2, ({host_a2})(a0)
    sd   a3, ({host_a3})(a0)
    sd   a4, ({host_a4})(a0)
    sd   a5, ({host_a5})(a0)
    sd   a6, ({host_a6})(a0)
    sd   a7, ({host_a7})(a0)
    sd   s2, ({host_s2})(a0)
    sd   s3, ({host_s3})(a0)
    sd   s4, ({host_s4})(a0)
    sd   s5, ({host_s5})(a0)
    sd   s6, ({host_s6})(a0)
    sd   s7, ({host_s7})(a0)
    sd   s8, ({host_s8})(a0)
    sd   s9, ({host_s9})(a0)
    sd   s10, ({host_s10})(a0)
    sd   s11, ({host_s11})(a0)
    sd   sp, ({host_sp})(a0)

    /* Swap in task CSRs. */
    ld    t1, ({task_sepc})(a0)
    csrw  sepc, t1

    /* Set stvec so that hypervisor resumes after the sret when the task exits. */
    la    t1, _task_exit
    csrrw t1, stvec, t1
    sd    t1, ({host_stvec})(a0)

    /* Save sscratch and replace with pointer to TaskInfo. */
    csrrw t1, sscratch, a0
    sd    t1, ({host_sscratch})(a0)

    /* Restore the gprs from this TaskInfo */
    ld   ra, ({task_ra})(a0)
    ld   gp, ({task_gp})(a0)
    ld   tp, ({task_tp})(a0)
    ld   s0, ({task_s0})(a0)
    ld   s1, ({task_s1})(a0)
    ld   a1, ({task_a1})(a0)
    ld   a2, ({task_a2})(a0)
    ld   a3, ({task_a3})(a0)
    ld   a4, ({task_a4})(a0)
    ld   a5, ({task_a5})(a0)
    ld   a6, ({task_a6})(a0)
    ld   a7, ({task_a7})(a0)
    ld   s2, ({task_s2})(a0)
    ld   s3, ({task_s3})(a0)
    ld   s4, ({task_s4})(a0)
    ld   s5, ({task_s5})(a0)
    ld   s6, ({task_s6})(a0)
    ld   s7, ({task_s7})(a0)
    ld   s8, ({task_s8})(a0)
    ld   s9, ({task_s9})(a0)
    ld   s10, ({task_s10})(a0)
    ld   s11, ({task_s11})(a0)
    ld   t0, ({task_t0})(a0)
    ld   t1, ({task_t1})(a0)
    ld   t2, ({task_t2})(a0)
    ld   t3, ({task_t3})(a0)
    ld   t4, ({task_t4})(a0)
    ld   t5, ({task_t5})(a0)
    ld   t6, ({task_t6})(a0)
    ld   sp, ({task_sp})(a0)
    ld   a0, ({task_a0})(a0)

    sret

.align 2
_task_exit:
    /* Pull TaskInfo out of sscratch, swapping with task's a0 */
    csrrw a0, sscratch, a0

    /* Save task GPRs. */
    sd   ra, ({task_ra})(a0)
    sd   gp, ({task_gp})(a0)
    sd   tp, ({task_tp})(a0)
    sd   s0, ({task_s0})(a0)
    sd   s1, ({task_s1})(a0)
    sd   a1, ({task_a1})(a0)
    sd   a2, ({task_a2})(a0)
    sd   a3, ({task_a3})(a0)
    sd   a4, ({task_a4})(a0)
    sd   a5, ({task_a5})(a0)
    sd   a6, ({task_a6})(a0)
    sd   a7, ({task_a7})(a0)
    sd   s2, ({task_s2})(a0)
    sd   s3, ({task_s3})(a0)
    sd   s4, ({task_s4})(a0)
    sd   s5, ({task_s5})(a0)
    sd   s6, ({task_s6})(a0)
    sd   s7, ({task_s7})(a0)
    sd   s8, ({task_s8})(a0)
    sd   s9, ({task_s9})(a0)
    sd   s10, ({task_s10})(a0)
    sd   s11, ({task_s11})(a0)
    sd   t0, ({task_t0})(a0)
    sd   t1, ({task_t1})(a0)
    sd   t2, ({task_t2})(a0)
    sd   t3, ({task_t3})(a0)
    sd   t4, ({task_t4})(a0)
    sd   t5, ({task_t5})(a0)
    sd   t6, ({task_t6})(a0)
    sd   sp, ({task_sp})(a0)

    /* Save Task a0 after recovering from sscratch. */
    csrr  t0, sscratch
    sd    t0, ({task_a0})(a0)

    /* Swap in host CSRs. */
    ld    t1, ({host_stvec})(a0)
    csrw  stvec, t1

    ld    t1, ({host_sscratch})(a0)
    csrw  sscratch, t1

    /* Save task EPC. */
    csrr  t1, sepc
    sd    t1, ({task_sepc})(a0)

    /* Restore hypervisor GPRs. */
    ld   ra, ({host_ra})(a0)
    ld   gp, ({host_gp})(a0)
    ld   tp, ({host_tp})(a0)
    ld   s0, ({host_s0})(a0)
    ld   s1, ({host_s1})(a0)
    ld   a1, ({host_a1})(a0)
    ld   a2, ({host_a2})(a0)
    ld   a3, ({host_a3})(a0)
    ld   a4, ({host_a4})(a0)
    ld   a5, ({host_a5})(a0)
    ld   a6, ({host_a6})(a0)
    ld   a7, ({host_a7})(a0)
    ld   s2, ({host_s2})(a0)
    ld   s3, ({host_s3})(a0)
    ld   s4, ({host_s4})(a0)
    ld   s5, ({host_s5})(a0)
    ld   s6, ({host_s6})(a0)
    ld   s7, ({host_s7})(a0)
    ld   s8, ({host_s8})(a0)
    ld   s9, ({host_s9})(a0)
    ld   s10, ({host_s10})(a0)
    ld   s11, ({host_s11})(a0)
    ld   sp, ({host_sp})(a0)

    ret
