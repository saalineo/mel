/* Linker script for MEL x86-64 Hello UART Boot Payload */
OUTPUT_ARCH("i386:x86-64")
ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x00100000, LENGTH = 16M
}

SECTIONS
{
    . = 0x00100000;

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
