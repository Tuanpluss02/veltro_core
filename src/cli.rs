use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "veltro", bin_name = "veltro", about = "Fast Dart code generation")]
pub struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan lib/ and generate all .g.dart files.
    Build {
        /// Enable verbose output.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Continuously watch lib/ for changes and rebuild.
    Watch,
    /// Delete all .g.dart files under lib/.
    Clean,
}
