# MEL Hardware Reference — arm64

Target Architecture: `Arm64`

- Reset Vector: `0x40000000`
- Default RAM Base: `0x40000000`
- Default Endianness: `Little`

## Memory Map Layout

| Address Range | Size | Component | Access | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x0000_0000 - 0x0000_FFFF` | 64 KB | Boot ROM | R/X | Reset vector (`0x0000_0000`) |
| `0x0800_0000 - 0x0800_FFFF` | 64 KB | GIC | MMIO R/W | Generic Interrupt Controller |
| `0x0900_0000 - 0x0900_00FF` | 256 B | UART | MMIO R/W | Serial Console (PL011) |
| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual Disk Controller |
| `0x4000_0000 - 0x43FF_FFFF` | 64 MB | Main RAM | R/W/X | System Physical RAM |

## ABI Register Conventions

| Register | Role |
| :--- | :--- |
| `x0..x7` | Parameter / Result registers |
| `x8` | Indirect result / Syscall ID |
| `x19..x28` | Callee-saved registers |
| `x29` (`fp`) | Frame pointer |
| `x30` (`lr`) | Link register |
| `sp` | Stack pointer |
