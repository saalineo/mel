/* Linker script for MEL RISC-V 64-bit Hello UART Boot Payload */
OUTPUT_ARCH("riscv")
ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 16M
}

SECTIONS
{
    . = 0x80000000;

    .text : {
        *(.text.boot)
        *(.text*)
    } > RAM

    .rodata : {
        *(.rodata*)
        *(.srodata*)
    } > RAM

    .data : {
        *(.data*)
        *(.sdata*)
    } > RAM

    .bss : {
        *(.bss*)
        *(.sbss*)
    } > RAM

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame)
    }
}
