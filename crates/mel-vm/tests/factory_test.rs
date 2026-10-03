use mel_core::profile::MachineProfile;
use mel_core::{Architecture, Endianness, MelError};
use mel_cpu::traits::CpuStepResult;
use mel_vm::factory::{
    build_vm_for_arch, build_vm_from_file, build_vm_from_profile, build_vm_from_yaml_str,
};
use std::path::Path;

#[test]
fn test_build_vm_from_yaml_files_all_architectures() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let machine_specs = [
        ("machines/riscv64_virt.yaml", Architecture::Riscv64, 0x8000_0000),
        ("machines/arm64_virt.yaml", Architecture::Arm64, 0x4000_0000),
        ("machines/x86_64_virt.yaml", Architecture::X86_64, 0x0010_0000),
        ("machines/ppc64_virt.yaml", Architecture::PowerIso64, 0x0000_0100),
    ];

    for (rel_path, expected_arch, expected_pc) in machine_specs {
        let file_path = workspace_root.join(rel_path);
        let mut vm = build_vm_from_file(&file_path)
            .unwrap_or_else(|e| panic!("Failed to build VM from '{}': {e}", file_path.display()));

        assert_eq!(vm.arch(), expected_arch);
        assert_eq!(vm.pc().unwrap(), expected_pc);

        // Verify UART device mapping
        assert!(vm.uart().is_some());

        // Verify RAM access through bus
        let ram_base = match expected_arch {
            Architecture::Riscv64 => 0x8000_0000,
            Architecture::Arm64 => 0x4000_0000,
            Architecture::X86_64 => 0x0010_0000,
            Architecture::PowerIso64 => 0x0000_0000,
        };

        vm.write_memory(ram_base, &[0xDE, 0xAD, 0xBE, 0xEF])
            .unwrap();
        let mut read_buf = [0u8; 4];
        vm.read_memory(ram_base, &mut read_buf).unwrap();
        assert_eq!(read_buf, [0xDE, 0xAD, 0xBE, 0xEF]);
    }
}

#[test]
fn test_build_vm_from_default_profiles_all_architectures() {
    for &arch in &Architecture::ALL {
        let vm = build_vm_for_arch(arch).unwrap();
        assert_eq!(vm.arch(), arch);

        let default_profile = MachineProfile::default_for_arch(arch);
        assert_eq!(vm.pc().unwrap(), default_profile.boot.entry_point);

        let ram_base = default_profile.effective_ram_base();
        vm.bus()
            .lock()
            .unwrap()
            .write(ram_base, 0xCAFEBABE, 4, Endianness::Little)
            .unwrap();

        assert_eq!(
            vm.bus()
                .lock()
                .unwrap()
                .read(ram_base, 4, Endianness::Little)
                .unwrap(),
            0xCAFEBABE
        );
    }
}

#[test]
fn test_build_vm_from_yaml_str_custom_profile() {
    let custom_yaml = r#"
name: custom-riscv64
description: Custom RISC-V 64 profile with 128 MB RAM
architecture: riscv64
ram_size: 134217728
ram_base: 0x80000000
boot:
  entry_point: 0x80000000
  reset_vector: 0x80000000
  rom:
    base: 0x00001000
    size: 0x1000
devices:
  uart:
    base: 0x10000000
    size: 0x0100
    irq: 10
  clint:
    base: 0x02000000
    size: 0x00010000
    harts: 1
  plic:
    base: 0x0C000000
    size: 0x04000000
    contexts: 1
    max_sources: 32
  block:
    base: 0x20000000
    size: 0x1000
    irq: 1
"#;

    let vm = build_vm_from_yaml_str(custom_yaml).unwrap();
    assert_eq!(vm.arch(), Architecture::Riscv64);
    assert_eq!(vm.pc().unwrap(), 0x8000_0000);
    assert_eq!(vm.ram().lock().unwrap().size(), 134217728);
    assert!(vm.uart().is_some());
    assert!(vm.clint().is_some());
    assert!(vm.plic().is_some());
    assert!(vm.block_device().is_some());
    assert!(vm.boot_rom().is_some());
}

#[test]
fn test_build_vm_invalid_profiles_rejected() {
    // 1. Zero RAM
    let mut profile = MachineProfile::riscv64_default();
    profile.ram_size = 0;
    assert!(matches!(
        build_vm_from_profile(&profile),
        Err(MelError::Config(_))
    ));

    // 2. Excessively large RAM (>16 GB)
    let mut profile = MachineProfile::arm64_default();
    profile.ram_size = 32 * 1024 * 1024 * 1024;
    assert!(matches!(
        build_vm_from_profile(&profile),
        Err(MelError::Config(_))
    ));

    // 3. Invalid YAML format
    assert!(matches!(
        build_vm_from_yaml_str("invalid: yaml: [content"),
        Err(MelError::Config(_))
    ));
}

#[test]
fn test_build_vm_multi_arch_instruction_execution() {
    // 1. RISC-V 64: Execute UART output write
    {
        let mut vm = build_vm_for_arch(Architecture::Riscv64).unwrap();
        // lui x1, 0x10000; addi x2, x0, 75 ('K'); sb x2, 0(x1)
        let code: [u8; 12] = [
            0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0xb0, 0x04, 0x23, 0x80, 0x20, 0x00,
        ];
        vm.write_memory(0x8000_0000, &code).unwrap();
        vm.set_pc(0x8000_0000).unwrap();

        for _ in 0..3 {
            let step_result = vm.step().unwrap();
            assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
        }

        let output = vm.uart().unwrap().lock().unwrap().drain_output();
        assert_eq!(output, b"K");
    }

    // 2. ARM64: Execute NOP (0xd503201f)
    {
        let mut vm = build_vm_for_arch(Architecture::Arm64).unwrap();
        let nop_inst = 0xd503201fu32.to_le_bytes();
        vm.write_memory(0x4000_0000, &nop_inst).unwrap();
        vm.set_pc(0x4000_0000).unwrap();

        let step_result = vm.step().unwrap();
        assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(vm.pc().unwrap(), 0x4000_0004);
    }

    // 3. x86-64: Execute NOP (0x90)
    {
        let mut vm = build_vm_for_arch(Architecture::X86_64).unwrap();
        vm.write_memory(0x0010_0000, &[0x90]).unwrap();
        vm.set_pc(0x0010_0000).unwrap();

        let step_result = vm.step().unwrap();
        assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(vm.pc().unwrap(), 0x0010_0001);
    }

    // 4. Power ISA: Execute NOP (ori 0, 0, 0 = 0x60000000)
    {
        let mut vm = build_vm_for_arch(Architecture::PowerIso64).unwrap();
        let nop_inst = 0x6000_0000u32.to_be_bytes();
        vm.write_memory(0x0000_0100, &nop_inst).unwrap();
        vm.set_pc(0x0000_0100).unwrap();

        let step_result = vm.step().unwrap();
        assert_eq!(step_result, CpuStepResult::Ok { cycles: 1 });
        assert_eq!(vm.pc().unwrap(), 0x0000_0104);
    }
}
