# MEL ARM64 Hello UART Boot Payload

This standalone bare-metal payload demonstrates power-on booting, stack pointer initialization, and character transmission across the NS16550 UART controller on the `arm64` (AArch64) architecture.

## Memory & Hardware Map

| Parameter | Value | Description |
| :--- | :--- | :--- |
| **Architecture** | `ARM64` / `AArch64` | ARM 64-bit |
| **RAM Base Address** | `0x4000_0000` | Code load address & entry point PC |
| **Initial Stack Pointer** | `0x4001_0000` | Top of initial boot stack |
| **UART MMIO Base** | `0x0900_0000` | NS16550 Transmitter Holding Register (THR) |

## Expected Output
```text
Hello MEL ARM64!
```

## Running with MEL CLI

```bash
# Execute payload on ARM64 VM
cargo run -p mel-cli -- run examples/arm64/hello_uart/boot.bin --arch arm64

# Inspect payload disassembly and hex dump
cargo run -p mel-cli -- inspect examples/arm64/hello_uart/boot.bin --arch arm64

# Debug payload interactively
cargo run -p mel-cli -- debug examples/arm64/hello_uart/boot.bin --arch arm64
```

## Rebuilding from Source (Optional)

If an ARM64 GNU or LLVM toolchain is installed on the host:

```bash
make
# or using clang:
clang --target=aarch64-linux-gnu -c boot.S -o boot.o
ld.lld -T linker.ld -o boot.elf boot.o
llvm-objcopy -O binary boot.elf boot.bin
```
