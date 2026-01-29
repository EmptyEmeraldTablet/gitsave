mod cli;
mod core;
mod error;
mod git;
mod manager;

use anyhow::{Context, Result};
use cli::{parse_args, Cli, Commands, RouteCommands};
use error::SaveError;
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

    println!("[OK] Initialized gitsave repository");
    println!("  Location: {}", save_dir.display());
    println!("  Git path: {}", repo.path().display());
    Ok(())
}

fn handle_save(save_dir: &Path, message: &str) -> Result<()> {
    let mut core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = SaveManager::new(core);

    let result = manager.save(message).context("Failed to save")?;
    println!("[OK] Save successful!");
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
        if saves.is_empty() {
            println!("  (no saves yet)");
        } else {
            for save in saves {
                let current = if save.is_current { " (current)" } else { "" };
                println!("  {} - {}{}", save.short_id, save.message, current);
            }
        }
        return Ok(());
    }

    if let Some(id) = identifier {
        if preview {
            println!("Would load save: {}", id);
            return Ok(());
        }

        match manager.into_core().checkout(id) {
            Ok(()) => println!("Loaded save: {}", id),
            Err(SaveError::SaveNotFound(target)) => {
                let all_saves = SaveManager::new(Git2Core::open(save_dir)?).list_saves()?;
                eprintln!("[ERROR] Save not found: {}", target);
                if !all_saves.is_empty() {
                    eprintln!("\nAvailable saves:");
                    for save in all_saves {
                        eprintln!("  {} - {}", save.short_id, save.message);
                    }
                    eprintln!("\nUse 'gitsave load --list' to see all saves");
                } else {
                    eprintln!("No saves available. Use 'gitsave save <message>' to create one.");
                }
                std::process::exit(1);
            }
            Err(e) => return Err(e).context("Failed to checkout"),
        }
    }
    Ok(())
}

fn handle_status(save_dir: &Path) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let manager = SaveManager::new(core);
    let status = manager.get_status().context("Failed to get status")?;

    println!("Status:");
    println!("  Current route: {}", status.current_route);

    if let Some(last) = &status.last_save {
        println!("  Last save: {} - {}", last.short_id, last.message);
    } else {
        println!("  No saves yet");
    }

    if status.has_uncommitted_changes {
        println!(
            "  Uncommitted changes: {} files",
            status.pending_changes.len()
        );
        for change in &status.pending_changes {
            let status_char = match change.status {
                core::ChangeStatus::Added => "+",
                core::ChangeStatus::Modified => "~",
                core::ChangeStatus::Deleted => "-",
            };
            println!("    {} {}", status_char, change.path);
        }
    } else {
        println!("  No uncommitted changes");
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

fn handle_route(save_dir: &Path, command: &Option<RouteCommands>) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = RouteManager::new(core);

    match command {
        Some(RouteCommands::List) => {
            let routes = manager.list_routes().context("Failed to list routes")?;
            println!("Routes:");
            for route in routes {
                let current = if route.is_current { " (current)" } else { "" };
                let last = route
                    .latest_save
                    .as_ref()
                    .map(|s| format!(" - {}", s.message))
                    .unwrap_or_else(|| String::from(""));
                println!("  {}{}{}", route.name, current, last);
            }
        }
        Some(RouteCommands::Create { name }) => {
            manager
                .create_route(name)
                .context("Failed to create route")?;
            println!("[OK] Created route: {}", name);
        }
        Some(RouteCommands::Switch { name, create }) => {
            let mut core = manager.into_core();
            if *create {
                match core.switch_create_route(name) {
                    Ok(()) => println!("[OK] Created and switched to route: {}", name),
                    Err(SaveError::Repository(_)) => {
                        core.create_route(name).context("Failed to create route")?;
                        core.switch_route(name).context("Failed to switch route")?;
                        println!("[OK] Created and switched to route: {}", name);
                    }
                    Err(e) => return Err(e).context("Failed to create and switch route"),
                }
            } else {
                match core.switch_route(name) {
                    Ok(()) => println!("[OK] Switched to route: {}", name),
                    Err(SaveError::Repository(_)) => {
                        let all_routes =
                            RouteManager::new(Git2Core::open(save_dir)?).list_routes()?;
                        eprintln!("[ERROR] Route not found: {}", name);
                        if !all_routes.is_empty() {
                            eprintln!("\nAvailable routes:");
                            for route in all_routes {
                                let current = if route.is_current { " (current)" } else { "" };
                                eprintln!("  {}{}", route.name, current);
                            }
                            eprintln!("\nUse 'gitsave route --list' to see all routes");
                        } else {
                            eprintln!("No routes available. Use 'gitsave route create <name>' to create one.");
                        }
                        std::process::exit(1);
                    }
                    Err(e) => return Err(e).context("Failed to switch route"),
                }
            }
        }
        Some(RouteCommands::Delete { name }) => {
            let all_routes = manager.list_routes().context("Failed to list routes")?;
            let route_exists = all_routes.iter().any(|r| r.name == *name);

            if !route_exists {
                eprintln!("[ERROR] Route not found: {}", name);
                if !all_routes.is_empty() {
                    eprintln!("\nAvailable routes:");
                    for route in all_routes {
                        let current = if route.is_current { " (current)" } else { "" };
                        eprintln!("  {}{}", route.name, current);
                    }
                }
                eprintln!("\nUse 'gitsave route --list' to see all routes");
                std::process::exit(1);
            }

            let is_current = all_routes
                .iter()
                .find(|r| r.name == *name)
                .map(|r| r.is_current)
                .unwrap_or(false);

            if is_current {
                eprintln!("[ERROR] Cannot delete current route '{}'", name);
                eprintln!("Switch to another route first with 'gitsave route switch <name>'");
                std::process::exit(1);
            }

            eprint!("[CONFIRM] Delete route '{}'? [y/N]: ", name);
            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);
            if input.trim().to_lowercase() != "y" {
                println!("Cancelled.");
                return Ok(());
            }

            match manager.delete_route(name) {
                Ok(()) => println!("[OK] Deleted route: {}", name),
                Err(e) => {
                    eprintln!("[ERROR] Failed to delete route: {}", e);
                    std::process::exit(1);
                }
            }
        }
        None => {
            let current_route = manager
                .get_current_route()
                .context("Failed to get current route")?;
            println!("Current route: {}", current_route);
            println!("  Use 'gitsave route --list' to see all routes");
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
        Commands::Tag {
            list,
            name,
            message,
        } => {
            if *list {
                match Git2Core::open(&save_dir) {
                    Ok(core) => {
                        let manager = SaveManager::new(core);
                        match manager.list_tags() {
                            Ok(tags) => {
                                if tags.is_empty() {
                                    println!("No tags found");
                                } else {
                                    println!("Tags:");
                                    for tag_name in tags {
                                        println!("  {}", tag_name);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: Failed to list tags: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to open repository: {}", e);
                        std::process::exit(1);
                    }
                }
            } else if let Some(tag_name) = name {
                match Git2Core::open(&save_dir) {
                    Ok(core) => {
                        let manager = SaveManager::new(core);
                        let msg = message.clone().unwrap_or_else(|| tag_name.clone());
                        if let Err(e) = manager.create_tag(&tag_name, &msg) {
                            eprintln!("Error: Failed to create tag: {}", e);
                            std::process::exit(1);
                        }
                        println!("Created tag: {}", tag_name);
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to open repository: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                println!(
                    "Use 'gitsave tag --list' to list tags or 'gitsave tag <name>' to create one"
                );
            }
        }
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
        Commands::Config { set } => {
            if let Some(key_value) = set {
                let parts: Vec<&str> = key_value.split('=').collect();
                if parts.len() != 2 {
                    eprintln!("[ERROR] Invalid format. Use 'key=value'");
                    eprintln!("Example: gitsave config set save.max_history=100");
                    std::process::exit(1);
                }
                let key = parts[0];
                let value = parts[1];

                let config_path = save_dir.join("gitsave.toml");
                let config = if config_path.exists() {
                    std::fs::read_to_string(&config_path)
                        .map_err(|e| SaveError::Config(e.to_string()))
                        .and_then(|s| {
                            toml::from_str(&s).map_err(|e| SaveError::Config(e.to_string()))
                        })
                        .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()))
                } else {
                    toml::Value::Table(toml::Table::new())
                };

                let new_value: toml::Value = if value.parse::<i64>().is_ok() {
                    toml::Value::Integer(value.parse().unwrap())
                } else if value.to_lowercase() == "true" || value.to_lowercase() == "false" {
                    toml::Value::Boolean(value.parse().unwrap())
                } else {
                    toml::Value::String(value.to_string())
                };

                let mut table = match config {
                    toml::Value::Table(t) => t,
                    _ => toml::Table::new(),
                };

                let (section, key) = if let Some((s, k)) = key.split_once('.') {
                    (s, k)
                } else {
                    ("save", key)
                };

                if !table.contains_key(section) {
                    table.insert(section.to_string(), toml::Value::Table(toml::Table::new()));
                }

                if let Some(toml::Value::Table(section_table)) = table.get_mut(section) {
                    section_table.insert(key.to_string(), new_value);
                }

                match toml::to_string_pretty(&toml::Value::Table(table)) {
                    Ok(content) => {
                        if let Err(e) = std::fs::write(&config_path, &content) {
                            eprintln!("[ERROR] Failed to write config: {}", e);
                            std::process::exit(1);
                        }
                        println!("[OK] Config updated: {} = {}", key, value);
                    }
                    Err(e) => {
                        eprintln!("[ERROR] Failed to serialize config: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                let config_path = save_dir.join("gitsave.toml");
                if !config_path.exists() {
                    println!("No config file found. Using defaults.");
                    println!("  save.max_history = 50");
                    println!("  save.compression = 6");
                    println!("  auto_save.enabled = false");
                } else {
                    match std::fs::read_to_string(&config_path) {
                        Ok(content) => {
                            println!("Configuration:");
                            println!("{}", content);
                        }
                        Err(e) => {
                            eprintln!("[ERROR] Failed to read config: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }
}
