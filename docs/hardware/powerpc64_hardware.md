# MEL Hardware Reference — powerpc64

Target Architecture: `PowerIso64`

- Reset Vector: `0x00000100`
- Default RAM Base: `0x00000000`
- Default Endianness: `Big`

## Memory Map Layout

| Address Range | Size | Component | Access | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x0000_0000 - 0x03FF_FFFF` | 64 MB | Main RAM | R/W/X | System Physical RAM (Vectors at 0x100) |
| `0x1000_0000 - 0x1000_00FF` | 256 B | UART 16550 | MMIO R/W | Serial Console |
| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual Disk Controller |
| `0x3000_0000 - 0x3000_FFFF` | 64 KB | OpenPIC | MMIO R/W | Interrupt Controller |

## ABI Register Conventions

| Register | Role |
| :--- | :--- |
| `r0` | Language specific / Syscall ID |
| `r1` | Stack pointer |
| `r3..r10` | Function parameter / return values |
| `r14..r31` | Callee-saved nonvolatile registers |
