use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gitsave")]
#[command(author = "Game Save Manager")]
#[command(version = "0.1.0")]
#[command(about = "Game save management tool powered by Git", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, value_name = "PATH")]
    pub save_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Initialize a new save repository")]
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    #[command(about = "Save current game state")]
    Save {
        #[arg(short, long)]
        message: Option<String>,

        #[arg(default_value = "")]
        desc: String,
    },

    #[command(about = "Load a saved game state")]
    Load {
        #[arg(short, long)]
        list: bool,

        #[arg(short, long)]
        preview: bool,

        #[arg(short, long)]
        force: bool,

        identifier: Option<String>,
    },

    #[command(about = "Manage routes (branches)")]
    Route {
        #[command(subcommand)]
        command: RouteCommands,
    },

    #[command(about = "Show current status")]
    Status,

    #[command(about = "Show save history")]
    History {
        #[arg(short, long)]
        verbose: bool,

        #[arg(short, long, value_name = "ROUTE")]
        route: Option<String>,
    },

    #[command(about = "Compare two saves")]
    Compare { save1: String, save2: String },

    #[command(about = "Create a tag")]
    Tag {
        name: String,
        message: Option<String>,
    },

    #[command(about = "Export save repository")]
    Export { path: PathBuf },

    #[command(about = "Import save repository")]
    Import { path: PathBuf },

    #[command(about = "Show configuration")]
    Config {
        #[arg(short, long)]
        set: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RouteCommands {
    #[command(about = "List all routes")]
    List,

    #[command(about = "Create a new route")]
    Create { name: String },

    #[command(about = "Switch to a route")]
    Switch { name: String },

    #[command(about = "Delete a route")]
    Delete { name: String },
}

pub fn parse_args() -> Cli {
    Cli::parse()
}
