// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

.option norvc

.section .text.start

// The entry point for the boot CPU.
.global _task_start
_task_start:

.option push
.option norelax
    la gp, _global_pointer
.option pop
    la sp, _stack_end

    la t1, task_main
    la ra, 1f
    jr t1

1:
    // ecall to exit
    ecall

