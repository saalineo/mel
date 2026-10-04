# MEL — Standalone x86-64 (AMD64) Hello UART Boot Payload
#
# Target Architecture: x86-64 (AMD64)
# RAM Base Address   : 0x00100000
# Stack Top Address  : 0x00200000
# UART Base Address  : 0x10000000 (NS16550 MMIO THR at offset 0)

.code64
.section .text.boot, "ax", @progbits
.globl _start

_start:
    # 1. Initialize Stack Pointer rsp = 0x00200000
    movabsq $0x00200000, %rsp

    # 2. Setup UART Base Address rdi = 0x10000000
    movabsq $0x10000000, %rdi

    # 3. Load address of string message into rsi (RIP-relative)
    lea     msg(%rip), %rsi

print_loop:
    # 4. Load next character byte and advance pointer
    movzbq  (%rsi), %rax
    test    %al, %al
    jz      halt_loop

    # 5. Write character to UART Transmitter Holding Register (THR)
    movb    %al, (%rdi)

    # 6. Advance message pointer and loop
    incq    %rsi
    jmp     print_loop

halt_loop:
    # 7. Low-power pause / halt loop
    pause
    jmp     halt_loop

.section .rodata
msg:
    .asciz "Hello MEL x86-64!\n"
