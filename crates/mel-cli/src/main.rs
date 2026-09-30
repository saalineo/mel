use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "mel",
    version,
    about = "MEL — Serious Systems-Engineering Simulation"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show information about available machines
    Machine {
        #[arg(short, long, default_value = "riscv64")]
        arch: String,
    },
    /// Run a payload on the virtual machine
    Run {
        /// Path to binary or ELF executable
        file: String,
        #[arg(short, long, default_value = "riscv64")]
        arch: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Machine { arch } => {
            println!("MEL Virtual Machine Target: {}", arch);
            println!("Supported architectures: riscv64, arm64, x86_64, powerpc64");
        }
        Commands::Run { file, arch } => {
            println!(
                "Starting simulation for '{}' on [{}] architecture...",
                file, arch
            );
        }
    }

    Ok(())
}
