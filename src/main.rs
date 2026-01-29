mod cli;
mod core;
mod error;
mod git;
mod manager;

use anyhow::{Context, Result};
use cli::{parse_args, Cli, Commands, RouteCommands};
use git::Git2Core;
use manager::{RouteManager, SaveManager};
use std::path::{Path, PathBuf};

fn get_save_dir(cli: &Cli) -> PathBuf {
    if let Some(path) = &cli.save_dir {
        path.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn handle_init(save_dir: &Path) -> Result<()> {
    let core = Git2Core::init(save_dir).context("Failed to init repository")?;
    let repo = core.repo();
    let config_content = r#"# gitsave configuration
[save]
max_history = 50
compression = 6

[auto_save]
enabled = false
"#;
    let config_path = save_dir.join("gitsave.toml");
    std::fs::write(config_path, config_content).context("Failed to write config")?;

    println!("Initialized gitsave repository at {}", save_dir.display());
    println!("Repository path: {}", repo.path().display());
    Ok(())
}

fn handle_save(save_dir: &Path, message: &str) -> Result<()> {
    let mut core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = SaveManager::new(core);

    let result = manager.save(message).context("Failed to save")?;
    println!("Save successful!");
    println!("  ID: {}", result.short_oid);
    println!("  Message: {}", result.message);
    println!("  Files changed: {}", result.changed_files);
    Ok(())
}

fn handle_load(
    save_dir: &Path,
    list: bool,
    preview: bool,
    _force: bool,
    identifier: &Option<String>,
) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let manager = SaveManager::new(core);

    if list {
        let saves = manager.list_saves().context("Failed to list saves")?;
        println!("Available saves:");
        for save in saves {
            let current = if save.is_current { " (current)" } else { "" };
            println!("  {} - {}{}", save.short_id, save.message, current);
        }
        return Ok(());
    }

    if let Some(id) = identifier {
        if preview {
            println!("Would load save: {}", id);
            return Ok(());
        }

        let mut core = manager.into_core();
        core.checkout(id).context("Failed to checkout")?;
        println!("Loaded save: {}", id);
    }
    Ok(())
}

fn handle_status(save_dir: &Path) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let manager = SaveManager::new(core);
    let status = manager.get_status().context("Failed to get status")?;

    println!("Current route: {}", status.current_route);

    if let Some(last) = &status.last_save {
        println!("Last save: {} - {}", last.short_id, last.message);
    } else {
        println!("No saves yet");
    }

    if status.has_uncommitted_changes {
        println!(
            "Uncommitted changes: {} files",
            status.pending_changes.len()
        );
        for change in &status.pending_changes {
            let status_char = match change.status {
                core::ChangeStatus::Added => "+",
                core::ChangeStatus::Modified => "~",
                core::ChangeStatus::Deleted => "-",
            };
            println!("  {} {}", status_char, change.path);
        }
    } else {
        println!("No uncommitted changes");
    }
    Ok(())
}

fn handle_history(save_dir: &Path, verbose: bool, _route: &Option<String>) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let manager = SaveManager::new(core);
    let history = manager.get_history().context("Failed to get history")?;

    for save in history {
        let marker = if save.is_current { "*" } else { " " };
        print!("{} {} - {}", marker, save.short_id, save.message);
        if verbose {
            print!(" [{}]", save.timestamp.format("%Y-%m-%d %H:%M:%S"));
        }
        println!();
    }
    Ok(())
}

fn handle_route(save_dir: &Path, command: &RouteCommands) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = RouteManager::new(core);

    match command {
        RouteCommands::List => {
            let routes = manager.list_routes().context("Failed to list routes")?;
            for route in routes {
                let current = if route.is_current { " (current)" } else { "" };
                let last = route
                    .latest_save
                    .as_ref()
                    .map(|s| format!(" - {}", s.message))
                    .unwrap_or_else(|| String::from(""));
                println!("{}{}{}", route.name, current, last);
            }
        }
        RouteCommands::Create { name } => {
            manager
                .create_route(name)
                .context("Failed to create route")?;
            println!("Created route: {}", name);
        }
        RouteCommands::Switch { name } => {
            let mut core = manager.into_core();
            core.switch_route(name).context("Failed to switch route")?;
            println!("Switched to route: {}", name);
        }
        RouteCommands::Delete { name } => {
            manager
                .delete_route(name)
                .context("Failed to delete route")?;
            println!("Deleted route: {}", name);
        }
    }
    Ok(())
}

fn handle_compare(save_dir: &Path, save1: &str, save2: &str) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let manager = SaveManager::new(core);
    let result = manager
        .compare(save1, save2)
        .context("Failed to compare saves")?;

    println!(
        "Comparing {} and {}",
        result.from.short_id, result.to.short_id
    );
    println!(
        "Additions: {}, Deletions: {}",
        result.additions, result.deletions
    );
    for file in result.changed_files {
        println!("  {}: +{} -{}", file.path, file.additions, file.deletions);
    }
    Ok(())
}

fn main() {
    let cli = parse_args();
    let save_dir = get_save_dir(&cli);

    match &cli.command {
        Commands::Init { path } => {
            if let Err(e) = handle_init(&path) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Save { message, desc } => {
            let msg = message.clone().unwrap_or_else(|| desc.clone());
            if let Err(e) = handle_save(&save_dir, &msg) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Load {
            list,
            preview,
            force,
            identifier,
        } => {
            if let Err(e) = handle_load(&save_dir, *list, *preview, *force, identifier) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Status => {
            if let Err(e) = handle_status(&save_dir) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::History { verbose, route } => {
            if let Err(e) = handle_history(&save_dir, *verbose, route) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Route { command } => {
            if let Err(e) = handle_route(&save_dir, command) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Compare { save1, save2 } => {
            if let Err(e) = handle_compare(&save_dir, save1, save2) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Tag { name, message } => match Git2Core::open(&save_dir) {
            Ok(mut core) => {
                let mut manager = SaveManager::new(core);
                let msg = message.clone().unwrap_or_else(|| name.clone());
                if let Err(e) = manager.save(&msg) {
                    eprintln!("Error: Failed to save: {}", e);
                    std::process::exit(1);
                }
                println!("Created tag: {}", name);
            }
            Err(e) => {
                eprintln!("Error: Failed to open repository: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Export { path } => {
            if let Err(e) = std::fs::copy(&save_dir, path) {
                eprintln!("Error: Failed to export: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Import { path } => {
            if let Err(e) = Git2Core::init(&save_dir) {
                eprintln!("Error: Failed to import: {}", e);
                std::process::exit(1);
            }
            println!("Imported from: {}", path.display());
        }
        Commands::Config { set: _ } => {
            println!("Config management not yet implemented");
        }
    }
}
