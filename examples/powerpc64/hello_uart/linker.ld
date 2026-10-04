/* Linker script for MEL Power ISA 64-bit Hello UART Boot Payload */
OUTPUT_ARCH("powerpc")
ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x00000100, LENGTH = 16M
}

SECTIONS
{
    . = 0x00000100;

    .text : {
        *(.text.boot)
        *(.text*)
    } > RAM

    .rodata : {
        *(.rodata*)
    } > RAM

    .data : {
        *(.data*)
    } > RAM

    .bss : {
        *(.bss*)
    } > RAM

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame)
    }
}
