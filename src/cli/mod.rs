use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gitsave")]
#[command(author = "Game Save Manager")]
#[command(version = "0.1.0")]
#[command(about = "Game save management tool powered by Git", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, value_name = "PATH")]
    pub save_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Initialize a new save repository")]
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        force: bool,
    },

    #[command(about = "Save current game state")]
    Save {
        #[arg(short, long)]
        message: Option<String>,

        #[arg(default_value = "")]
        desc: String,
    },

    #[command(about = "Rename the latest save message (HEAD only)")]
    Amend {
        #[arg(short, long)]
        message: Option<String>,

        #[arg(default_value = "")]
        desc: String,
    },

    #[command(about = "Roll back to a saved game state (creates a new route)")]
    Load {
        #[arg(short, long)]
        list: bool,

        #[arg(short, long)]
        preview: bool,

        #[arg(short, long)]
        force: bool,

        #[arg(short, long, value_name = "TAG")]
        tag: Option<String>,

        #[arg(short, long, value_name = "ROUTE")]
        route: Option<String>,

        identifier: Option<String>,
    },

    #[command(about = "Manage routes (branches)")]
    Route {
        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        list: bool,

        #[command(subcommand)]
        command: Option<RouteCommands>,
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
        #[arg(short, long)]
        list: bool,

        #[arg(short, long)]
        delete: bool,

        name: Option<String>,
        message: Option<String>,
    },

    #[command(about = "Export save repository")]
    Export { path: PathBuf },

    #[command(about = "Import save repository")]
    Import { path: PathBuf },

    #[command(about = "Show or set configuration")]
    Config {
        #[arg(short, long)]
        set: Option<String>,
    },

    #[command(about = "Configure auto-save settings")]
    Autosave {
        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        enable: bool,

        #[arg(short, long, value_name = "SECONDS")]
        interval: Option<u64>,

        #[arg(short, long, value_name = "COUNT")]
        max_count: Option<u32>,

        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        status: bool,

        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        disable: bool,
    },

    #[command(about = "Launch the interactive TUI dashboard (experimental)")]
    Tui,

    #[cfg(feature = "gui")]
    #[command(about = "Launch the interactive GUI dashboard")]
    Gui,

    #[command(about = "Recovery mode (saved discards)")]
    Recovery {
        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        list: bool,

        #[arg(short, long, value_name = "NAME")]
        name: Option<String>,

        identifier: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RouteCommands {
    #[command(about = "List all routes")]
    List,

    #[command(about = "Create a new route")]
    Create { name: String },

    #[command(about = "Switch to a route")]
    Switch {
        name: String,

        #[arg(short, long)]
        create: bool,
    },

    #[command(about = "Delete a route")]
    Delete { name: String },

    #[command(about = "Rename a route")]
    Rename { old_name: String, new_name: String },
}

pub fn parse_args() -> Cli {
    Cli::parse()
}
