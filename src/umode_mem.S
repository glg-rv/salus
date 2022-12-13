// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

// Very unoptimized memcpy() to/from user memory functions.
// TODO: Optimize.
        
// Adds the instruction at 'lbl' to the exception table.
.macro add_extable lbl
.pushsection .extable, "a"
.balign      8
.quad        \lbl
.popsection
.endm

.section text

// memcpy() to a user address.
.global _copy_to_user
_copy_to_user:
        // handle_trap assumes t0 holds the address of where we want to jump to when we encounter
        // a fault and will stick SCAUSE in t1.
        la    t0, _ret_from_copy
        // _ret_from_copy assumes the return value is in t2.
        mv    t2, zero
1:
        beq   t2, a2, _ret_from_copy
        lb    t3, (a1)
2:
        sb t3, (a0)
        add_extable 2b
        addi  a0, a0, 1
        addi  a1, a1, 1
        addi  t2, t2, 1
        j     1b

// memcpy() from a user address.
.global _copy_from_guest
_copy_from_user:
        // handle_trap assumes t0 holds the address of where we want to jump to when we encounter
        // a fault and will stick SCAUSE in t1.
        la    t0, _ret_from_copy
        // _ret_from_copy assumes the return value is in t2.
        mv    t2, zero
1:
        beq   t2, a2, _ret_from_copy
2:
        lb t3, (a1)
        add_extable 2b
        sb    t3, (a0)
        addi  a0, a0, 1
        addi  a1, a1, 1
        addi  t2, t2, 1
        j     1b

// zeroes user_address
.global _bzero_user
_bzero_user:
        // handle_trap assumes t0 holds the address of where we want to jump to when we encounter
        // a fault and will stick SCAUSE in t1.
        la    t0, _ret_from_copy
        // _ret_from_copy assumes the return value is in t2.
        mv    t2, zero
1:
        beq   t2, a1, _ret_from_copy
2:
        sb zero, (a0)
        add_extable 2b
        addi  a0, a0, 1
        addi  t2, t2, 1
        j     1b

        .align 2
_ret_from_copy:
        mv    a0, t2
        ret
