use clap::Parser;
use mel_cli::{execute_debug, execute_inspect, execute_machine, execute_run, Cli, Commands};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match &cli.command {
        Commands::Run(args) => {
            let mut stdout = std::io::stdout();
            execute_run(args, &mut stdout)?;
        }
        Commands::Debug(args) => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            execute_debug(args, stdin.lock(), stdout)?;
        }
        Commands::Inspect(args) => {
            let mut stdout = std::io::stdout();
            execute_inspect(args, &mut stdout)?;
        }
        Commands::Machine(args) => {
            let mut stdout = std::io::stdout();
            execute_machine(args, &mut stdout)?;
        }
    }

    Ok(())
}
