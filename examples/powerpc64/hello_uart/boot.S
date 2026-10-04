# MEL — Standalone Power ISA 64-bit Hello UART Boot Payload
#
# Target Architecture: Power ISA (PowerPC 64-bit)
# RAM Base Address   : 0x00000000
# Entry Point PC     : 0x00000100
# Stack Top Address  : 0x00010000
# UART Base Address  : 0x10000000 (NS16550 THR at offset 0)

.section .text.boot, "ax", @progbits
.globl _start

_start:
    # 1. Initialize Stack Pointer r1 = 0x00010000
    lis     1, 1
    ori     1, 1, 0

    # 2. Setup UART Base Address r3 = 0x10000000
    lis     3, 0x1000
    ori     3, 3, 0

    # 3. Load address of string message into r4
    lis     4, msg@ha
    addi    4, 4, msg@l

print_loop:
    # 4. Load next character byte
    lbz     5, 0(4)
    # Check for null terminator (cmpwi r5, 0)
    cmpwi   5, 0
    beq     halt_loop

    # 5. Write character to UART Transmitter Holding Register (THR)
    stb     5, 0(3)

    # 6. Advance message pointer and loop
    addi    4, 4, 1
    b       print_loop

halt_loop:
    # 7. Spin loop
    b       halt_loop

.section .rodata
msg:
    .string "Hello MEL PowerPC!\n"
