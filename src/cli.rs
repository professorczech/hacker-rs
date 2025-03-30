use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hacker-rs")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Execute a query
    Run {
        query: String,
        
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Start interactive session
    Interactive,
}