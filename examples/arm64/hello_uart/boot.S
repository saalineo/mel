// MEL — Standalone ARM64 (AArch64) Hello UART Boot Payload
//
// Target Architecture: ARM64 (AArch64)
// RAM Base Address   : 0x40000000
// Stack Top Address  : 0x40010000
// UART Base Address  : 0x09000000 (NS16550 THR at offset 0)

.section .text.boot, "ax", @progbits
.globl _start

_start:
    // 1. Initialize Stack Pointer sp = 0x40010000
    movz    x0, #0x4001, lsl #16
    mov     sp, x0

    // 2. Setup UART Base Address x1 = 0x09000000
    movz    x1, #0x0900, lsl #16

    // 3. Load address of string message into x2
    adr     x2, msg

print_loop:
    // 4. Load next character byte and advance pointer
    ldrb    w3, [x2], #1
    // Check for null-terminator (0 byte)
    cbz     w3, halt_loop

    // 5. Write character to UART Transmitter Holding Register (THR)
    strb    w3, [x1]

    // 6. Loop back
    b       print_loop

halt_loop:
    // 7. Low-power wait for event / interrupt or spin
    wfe
    b       halt_loop

.section .rodata
msg:
    .asciz "Hello MEL ARM64!\n"
