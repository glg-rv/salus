// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

.option norvc

.section .text.start

// The entry point for a task.
.global _start
_start:

.option push
.option norelax
    la gp, __global_pointer$
.option pop
    la sp, _stack_end

    la t1, _init_args
    sd a0, 0(t1)
    sd a1, 8(t1)
    sd a2, 16(t1)
    sd a3, 24(t1)
    sd a4, 32(t1)
    sd a5, 40(t1)
    sd a6, 48(t1)
    sd a7, 56(t1)
    mv a0, t1
    li a1, 8
    call _libuser_start
1:
    // ecall to panic
    mv a0, zero
    ecall


.section .bss
.global _init_args
_init_args:     
        .skip 64
