OUTPUT_ARCH( "riscv" )

ENTRY( _start )

MEMORY
{
    ram  (wxa) : ORIGIN = 0xffffffff00000000, LENGTH = 128M
}

PHDRS
{
    text PT_LOAD;
    rodata PT_LOAD;
    data PT_LOAD;
}

SECTIONS
{
    .start : ALIGN(4096) {
        *(.text.start)
        *(.text.init) *(.text .text.*)
    } >ram AT>ram :text

    .rodata : ALIGN(4096) {
        *(.rodata .rodata.*)
    } >ram AT>ram :rodata

    .data : ALIGN(4096) {
        *(.data .data.*)

        . = ALIGN(8);
        PROVIDE(__global_pointer$ = .);

        *(.sdata .sdata.*)

        *(.sbss .sbss.*) *(.bss .bss.*)
        PROVIDE(_bss_end = .); 
    } >ram AT>ram :data

    .stack : ALIGN(4096) {
        PROVIDE(_stack_start = .);
        . += 4096;
        PROVIDE(_stack_end = .);
    } >ram AT>ram :data

    /DISCARD/ : {
        *(.eh_frame)
    }
}
