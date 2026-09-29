//! Raw flat binary payload loader.

use std::path::Path;
use mel_core::MelError;
use mel_vm::VirtualMachine;

/// Load a raw binary payload into VM memory and update the CPU program counter.
pub fn load_raw_binary(
    vm: &mut VirtualMachine,
    data: &[u8],
    target_addr: u64,
) -> Result<(), MelError> {
    if data.is_empty() {
        return Ok(());
    }

    let end_addr = target_addr
        .checked_add(data.len() as u64)
        .ok_or_else(|| {
            MelError::Loader(format!(
                "Payload size {} at target address {:#x} overflows address space",
                data.len(),
                target_addr
            ))
        })?;

    // Write payload into memory and point program counter at entry address
    vm.write_memory(target_addr, data)?;
    vm.set_pc(target_addr)?;

    tracing::debug!(
        "Loaded raw binary ({} bytes) at {:#x}..{:#x}, entry PC set to {:#x}",
        data.len(),
        target_addr,
        end_addr,
        target_addr
    );

    Ok(())
}

/// Load a raw binary file from disk into VM memory.
pub fn load_raw_binary_from_file<P: AsRef<Path>>(
    vm: &mut VirtualMachine,
    path: P,
    target_addr: u64,
) -> Result<(), MelError> {
    let payload = std::fs::read(path)?;
    load_raw_binary(vm, &payload, target_addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_core::Architecture;
    use mel_cpu::traits::CpuStepResult;
    use mel_vm::{create_basic_riscv64_vm, create_mock_vm};

    #[test]
    fn test_load_raw_binary_mock_vm() {
        let mut vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let payload = [0x13, 0x05, 0xa0, 0x02]; // addi a0, zero, 42
        let target_addr = 0x8000_0000;

        load_raw_binary(&mut vm, &payload, target_addr).unwrap();
        assert_eq!(vm.pc().unwrap(), target_addr);

        let mut read_back = [0u8; 4];
        vm.read_memory(target_addr, &mut read_back).unwrap();
        assert_eq!(read_back, payload);
    }

    #[test]
    fn test_load_raw_binary_unmapped_target() {
        let mut vm = create_mock_vm(Architecture::Riscv64).unwrap();
        let payload = [0xAA, 0xBB, 0xCC, 0xDD];
        let unmapped_addr = 0x5000_0000;

        let result = load_raw_binary(&mut vm, &payload, unmapped_addr);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_raw_binary_and_execute_riscv64() {
        let mut vm = create_basic_riscv64_vm().unwrap();
        // lui x1, 0x10000 -> addi x2, x0, 89 ('Y') -> sb x2, 0(x1)
        let payload: [u8; 12] = [
            0xb7, 0x00, 0x00, 0x10,
            0x13, 0x01, 0x90, 0x05,
            0x23, 0x80, 0x20, 0x00,
        ];
        let entry_point = 0x8000_0000;

        load_raw_binary(&mut vm, &payload, entry_point).unwrap();
        assert_eq!(vm.pc().unwrap(), entry_point);

        let execution = vm.run(3).unwrap();
        assert_eq!(execution, CpuStepResult::Ok { cycles: 3 });

        let serial_out = vm.uart().unwrap().lock().unwrap().drain_output();
        assert_eq!(serial_out, b"Y");
    }
}
