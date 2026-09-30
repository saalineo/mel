use clap::Parser;
use mel_cli::{
    execute_debug, execute_inspect, execute_machine, execute_run, ArchArg, Cli, Commands,
    DebugArgs, InspectArgs, MachineArgs, MachineSubcommand, RunArgs,
};

#[test]
fn test_cli_arg_parsing() {
    // 1. mel run
    let cli = Cli::try_parse_from([
        "mel",
        "run",
        "boot.bin",
        "--arch",
        "riscv64",
        "--cycles",
        "50000",
        "--entry",
        "0x8000_0000",
    ])
    .unwrap();

    match cli.command {
        Commands::Run(args) => {
            assert_eq!(args.file, "boot.bin");
            assert_eq!(args.arch, ArchArg::Riscv64);
            assert_eq!(args.cycles, 50000);
            assert_eq!(args.entry, Some(0x8000_0000));
            assert!(!args.quiet);
        }
        _ => panic!("Expected Run command"),
    }

    // 2. mel debug
    let cli_dbg = Cli::try_parse_from([
        "mel",
        "debug",
        "kernel.bin",
        "--arch",
        "arm64",
        "--entry",
        "0x40000000",
    ])
    .unwrap();

    match cli_dbg.command {
        Commands::Debug(args) => {
            assert_eq!(args.file, "kernel.bin");
            assert_eq!(args.arch, ArchArg::Arm64);
            assert_eq!(args.entry, Some(0x4000_0000));
        }
        _ => panic!("Expected Debug command"),
    }

    // 3. mel inspect
    let cli_insp = Cli::try_parse_from(["mel", "inspect", "payload.bin", "-n", "25"]).unwrap();

    match cli_insp.command {
        Commands::Inspect(args) => {
            assert_eq!(args.file, "payload.bin");
            assert_eq!(args.count, 25);
        }
        _ => panic!("Expected Inspect command"),
    }

    // 4. mel machine list
    let cli_mach = Cli::try_parse_from(["mel", "machine", "list"]).unwrap();
    match cli_mach.command {
        Commands::Machine(args) => match args.command {
            Some(MachineSubcommand::List) => {}
            _ => panic!("Expected Machine List subcommand"),
        },
        _ => panic!("Expected Machine command"),
    }
}

#[test]
fn test_cli_machine_list_and_info() {
    // List
    let mut list_buf = Vec::new();
    let args = MachineArgs {
        command: Some(MachineSubcommand::List),
    };
    execute_machine(&args, &mut list_buf).unwrap();
    let list_str = String::from_utf8(list_buf).unwrap();

    assert!(list_str.contains("Supported CPU Architectures"));
    assert!(list_str.contains("riscv64"));
    assert!(list_str.contains("arm64"));
    assert!(list_str.contains("x86_64"));
    assert!(list_str.contains("powerpc64"));

    // Info
    let mut info_buf = Vec::new();
    let info_args = MachineArgs {
        command: Some(MachineSubcommand::Info {
            arch: ArchArg::Riscv64,
        }),
    };
    execute_machine(&info_args, &mut info_buf).unwrap();
    let info_str = String::from_utf8(info_buf).unwrap();

    assert!(info_str.contains("Architecture Details: riscv64"));
    assert!(info_str.contains("Boot ROM"));
    assert!(info_str.contains("UART 16550"));
    assert!(info_str.contains("Main RAM"));
}

#[test]
fn test_cli_run_and_inspect_execution() {
    let temp_dir = std::env::temp_dir();
    let payload_path = temp_dir.join("test_mel_cli_payload.bin");

    // 4 RISC-V instructions:
    // lui x1, 0x10000 -> addi x2, x0, 90 ('Z') -> sb x2, 0(x1) -> j . (jal x0, 0)
    let code: [u8; 16] = [
        0xb7, 0x00, 0x00, 0x10, 0x13, 0x01, 0xa0, 0x05, 0x23, 0x80, 0x20, 0x00, 0x6f, 0x00, 0x00,
        0x00,
    ];
    std::fs::write(&payload_path, code).unwrap();

    // 1. Inspect
    let mut inspect_buf = Vec::new();
    let inspect_args = InspectArgs {
        file: payload_path.to_str().unwrap().to_string(),
        arch: ArchArg::Riscv64,
        entry: Some(0x8000_0000),
        count: 5,
    };
    execute_inspect(&inspect_args, &mut inspect_buf).unwrap();
    let inspect_str = String::from_utf8(inspect_buf).unwrap();

    assert!(inspect_str.contains("Payload Inspection"));
    assert!(inspect_str.contains("lui"));
    assert!(inspect_str.contains("addi"));
    assert!(inspect_str.contains("sb"));

    // 2. Run
    let mut run_buf = Vec::new();
    let run_args = RunArgs {
        file: payload_path.to_str().unwrap().to_string(),
        arch: ArchArg::Riscv64,
        cycles: 10,
        entry: Some(0x8000_0000),
        quiet: false,
    };
    execute_run(&run_args, &mut run_buf).unwrap();
    let run_str = String::from_utf8(run_buf).unwrap();

    assert!(run_str.contains("[MEL] Running"));
    assert!(run_str.contains("[UART Console Output]:"));
    assert!(run_str.contains('Z'));
    assert!(run_str.contains("Execution stopped"));

    // 3. Debug
    let debug_args = DebugArgs {
        file: payload_path.to_str().unwrap().to_string(),
        arch: ArchArg::Riscv64,
        entry: Some(0x8000_0000),
    };
    let input_script = b"step 3\nregs\nquit\n";
    let mut debug_buf = Vec::new();
    execute_debug(&debug_args, &input_script[..], &mut debug_buf).unwrap();
    let debug_str = String::from_utf8(debug_buf).unwrap();

    assert!(debug_str.contains("MEL Interactive Debugger"));
    assert!(debug_str.contains("Single step completed"));
    assert!(debug_str.contains("Architecture: Riscv64"));

    // Clean up
    let _ = std::fs::remove_file(payload_path);
}
