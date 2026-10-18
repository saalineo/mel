# MEL RISC-V 64 Hello UART Boot Payload

This standalone bare-metal payload demonstrates power-on booting, stack pointer initialization, and character transmission across the NS16550 UART controller on the `riscv64` architecture.

## Memory & Hardware Map

| Parameter | Value | Description |
| :--- | :--- | :--- |
| **Architecture** | `RV64GC` | RISC-V 64-bit |
| **RAM Base Address** | `0x8000_0000` | Code load address & entry point PC |
| **Initial Stack Pointer** | `0x8001_0000` | Top of initial boot stack |
| **UART MMIO Base** | `0x1000_0000` | NS16550 Transmitter Holding Register (THR) |

## Expected Output
```text
Hello MEL RISC-V!
```

## Running with MEL CLI

```bash
# Execute payload on RISC-V VM
cargo run -p mel-cli -- run examples/riscv64/hello_uart/boot.bin --arch riscv64

# Inspect payload disassembly and hex dump
cargo run -p mel-cli -- inspect examples/riscv64/hello_uart/boot.bin --arch riscv64

# Debug payload interactively
cargo run -p mel-cli -- debug examples/riscv64/hello_uart/boot.bin --arch riscv64
```

## Rebuilding from Source (Optional)

If a RISC-V GNU toolchain (`riscv64-unknown-elf-as`, `riscv64-unknown-elf-ld`, `riscv64-unknown-elf-objcopy`) is installed on the host:

```bash
make
```
