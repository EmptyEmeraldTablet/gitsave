// Hide the console window on Windows when running the GUI build.
#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]

mod cli;
mod cache;
mod core;
mod error;
mod git;
mod manager;
mod state;
mod tui;
#[cfg(feature = "gui")]
mod gui;

use anyhow::{Context, Result};
use cli::{Cli, Commands, RouteCommands, parse_args};
use error::SaveError;
use cache::AutoSaveStateCache;
use git::Git2Core;
use manager::{ConfigManager, RouteManager, SaveManager, is_recovery_branch_name};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(not(feature = "gui"))]
use clap::CommandFactory;

// On Windows the process is linked as a GUI subsystem (no console), so we call
// MessageBoxW directly to surface fatal errors to the user.
#[cfg(windows)]
unsafe extern "system" {
    fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, utype: u32) -> i32;
}

#[cfg(windows)]
fn windows_message_box(caption: *const u16, text: *const u16) {
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text, caption, 0x10 /* MB_ICONERROR */);
    }
}

fn get_save_dir(cli: &Cli) -> PathBuf {
    if let Some(path) = &cli.save_dir {
        path.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

const DEFAULT_COMPRESSION: i32 = 6;

fn handle_init(save_dir: &Path, force: bool) -> Result<()> {
    if let Ok(existing) = Git2Core::open(save_dir) {
        let config_path = existing.workdir().join("gitsave.toml");
        if config_path.exists() {
            if !force {
                eprintln!(
                    "[ERROR] gitsave repository already exists at {}. Use --force to re-init.",
                    existing.workdir().display()
                );
                std::process::exit(1);
            }
        } else {
            eprintln!(
                "[ERROR] A non-gitsave Git repository exists at {}. Refusing to init here.",
                existing.workdir().display()
            );
            eprintln!("Tip: choose a dedicated save folder or remove the existing .git directory.");
            std::process::exit(1);
        }
    }

    let mut core = Git2Core::init(save_dir).context("Failed to init repository")?;
    core
        .set_core_compression(DEFAULT_COMPRESSION)
        .context("Failed to set core.compression")?;
    let config_content = format!(
        "# gitsave configuration\n[save]\nmax_history = 50\ncompression = {}\n\n[auto_save]\nenabled = false\n\n[author]\nname = \"\"\nemail = \"\"\n",
        DEFAULT_COMPRESSION
    );

    let config_path = save_dir.join("gitsave.toml");
    std::fs::write(&config_path, config_content).context("Failed to write config")?;

    let attributes_path = save_dir.join(".gitattributes");
    let attributes_content = "# Treat game saves as binary\nsaves/** -text -diff -merge\n";
    std::fs::write(&attributes_path, attributes_content)
        .context("Failed to write .gitattributes")?;

    core
        .commit_files(
            &[config_path.clone(), attributes_path.clone()],
            "init gitsave config",
        )
        .context("Failed to create initial config commit")?;

    println!("[OK] Initialized gitsave repository");
    println!("  Location: {}", save_dir.display());
    println!("  Git path: {}", core.repo().path().display());
    Ok(())
}

fn handle_autosave(
    save_dir: &Path,
    enable: bool,
    interval: Option<u64>,
    max_count: Option<u32>,
    status: bool,
    disable: bool,
) {
    let config_manager = ConfigManager::new(save_dir);
    let mut config = config_manager.load_auto_save_config();

    if status {
        println!("Auto-save configuration:");
        println!("  Enabled: {}", if config.enabled { "yes" } else { "no" });
        println!("  Interval: {} seconds", config.interval);
        println!("  Max count: {}", config.max_count);
        let last = AutoSaveStateCache::new().load_last_save_time(save_dir);
        if let Some(last) = last {
            let last_time = chrono::DateTime::from_timestamp(last, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("  Last auto-save: {}", last_time);
        } else {
            println!("  Last auto-save: never");
        }
        return;
    }

    if disable {
        config.enabled = false;
        if let Err(e) = config_manager.save_auto_save_config(&config) {
            eprintln!("[ERROR] Failed to disable auto-save: {}", e);
            std::process::exit(1);
        }
        println!("[OK] Auto-save disabled");
        return;
    }

    let mut changed = false;
    if enable {
        config.enabled = true;
        changed = true;
    }
    if let Some(interval_val) = interval {
        if interval_val < 60 {
            eprintln!("[ERROR] Interval must be at least 60 seconds");
            std::process::exit(1);
        }
        config.interval = interval_val;
        changed = true;
    }
    if let Some(max_count_val) = max_count {
        if max_count_val == 0 || max_count_val > 100 {
            eprintln!("[ERROR] Max count must be between 1 and 100");
            std::process::exit(1);
        }
        config.max_count = max_count_val;
        changed = true;
    }

    if changed {
        if let Err(e) = config_manager.save_auto_save_config(&config) {
            eprintln!("[ERROR] Failed to save auto-save config: {}", e);
            std::process::exit(1);
        }
        println!("[OK] Auto-save configuration updated:");
        println!("  Enabled: {}", if config.enabled { "yes" } else { "no" });
        println!("  Interval: {} seconds", config.interval);
        println!("  Max count: {}", config.max_count);
    } else {
        println!("No changes specified. Use:");
        println!("  gitsave autosave --enable           # Enable auto-save");
        println!("  gitsave autosave --disable          # Disable auto-save");
        println!("  gitsave autosave --interval 300     # Set interval in seconds");
        println!("  gitsave autosave --max_count 10     # Set max auto-saves to keep");
        println!("  gitsave autosave --status           # Show current settings");
    }
}

fn handle_save(save_dir: &Path, message: &str) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = SaveManager::new(core);

    let result = perform_stable_save_interactive(&mut manager, message)?;
    if let Some(result) = result {
        manager.update_last_save_time();
        println!("[OK] Save successful!");
        println!("  ID: {}", result.short_oid);
        println!("  Message: {}", result.message);
        println!("  Files changed: {}", result.changed_files);
    } else {
        println!("Cancelled.");
    }
    Ok(())
}

fn handle_amend(save_dir: &Path, message: &str) -> Result<()> {
    if message.trim().is_empty() {
        eprintln!("[ERROR] Message cannot be empty.");
        std::process::exit(1);
    }

    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = SaveManager::new(core);
    let status = manager.get_status()?;
    if status.has_uncommitted_changes {
        eprintln!("[ERROR] Working tree dirty. Save or discard changes first.");
        std::process::exit(1);
    }

    let result = manager
        .amend_head_message(message)
        .context("Failed to amend latest save")?;
    println!("[OK] Updated latest save message:");
    println!("  ID: {}", result.short_oid);
    println!("  Message: {}", result.message);
    Ok(())
}

fn handle_load(
    save_dir: &Path,
    list: bool,
    preview: bool,
    force: bool,
    tag: &Option<String>,
    route: &Option<String>,
    identifier: &Option<String>,
) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = SaveManager::new(core);

    if list {
        let mut saves = manager.list_saves().context("Failed to list saves")?;
        saves.retain(|save| !is_recovery_branch_name(&save.route));
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

    if let Some(tag_name) = tag {
        if preview {
            println!("Would roll back to tag: {}", tag_name);
            return Ok(());
        }

        let route_name = resolve_route_name(route, &format!("roll back to tag {}", tag_name))?;
        let Some(route_name) = route_name else {
            println!("Cancelled.");
            return Ok(());
        };

        let status = manager.get_status()?;
        if status.has_uncommitted_changes {
            if force {
                if !confirm_discard_changes(
                    "Uncommitted changes detected. Rolling back will discard them. Proceed?",
                )? {
                    println!("Cancelled.");
                    return Ok(());
                }
            } else if !ensure_clean_for_action(&mut manager, "rolling back to tag")? {
                println!("Cancelled.");
                return Ok(());
            }
        }

        let mut core = manager.into_core();
        match core.switch_create_route_at_tag(tag_name, &route_name) {
            Ok(()) => println!("Rolled back tag {} on route {}", tag_name, route_name),
            Err(e) => {
                eprintln!("[ERROR] Failed to roll back tag '{}': {}", tag_name, e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    if let Some(id) = identifier {
        if preview {
            println!("Would roll back to save: {}", id);
            return Ok(());
        }

        let route_name = resolve_route_name(route, &format!("roll back to save {}", id))?;
        let Some(route_name) = route_name else {
            println!("Cancelled.");
            return Ok(());
        };

        let status = manager.get_status()?;
        if status.has_uncommitted_changes {
            if force {
                if !confirm_discard_changes(
                    "Uncommitted changes detected. Rolling back will discard them. Proceed?",
                )? {
                    println!("Cancelled.");
                    return Ok(());
                }
            } else if !ensure_clean_for_action(&mut manager, "rolling back to save")? {
                println!("Cancelled.");
                return Ok(());
            }
        }

        match manager.into_core().switch_create_route_at(id, &route_name) {
            Ok(()) => println!("Rolled back to save {} on route {}", id, route_name),
            Err(SaveError::SaveNotFound(target)) => {
                let mut all_saves =
                    SaveManager::new(Git2Core::open(save_dir)?).list_saves()?;
                all_saves.retain(|save| !is_recovery_branch_name(&save.route));
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
    let mut history = manager.get_history().context("Failed to get history")?;
    history.retain(|save| !is_recovery_branch_name(&save.route));

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

fn list_recovery_routes(save_dir: &Path) -> Result<Vec<core::RouteInfo>> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let manager = RouteManager::new(core);
    let routes = manager.list_routes().context("Failed to list routes")?;
    Ok(routes
        .into_iter()
        .filter(|route| is_recovery_branch_name(&route.name))
        .collect())
}

fn resolve_recovery_route<'a>(
    routes: &'a [core::RouteInfo],
    input: &str,
) -> Result<&'a core::RouteInfo> {
    let matches: Vec<&core::RouteInfo> = routes
        .iter()
        .filter(|route| route.name.starts_with(input))
        .collect();
    match matches.len() {
        0 => {
            eprintln!("[ERROR] Recovery route not found: {}", input);
            std::process::exit(1);
        }
        1 => Ok(matches[0]),
        _ => {
            eprintln!("[ERROR] Multiple recovery routes match '{}':", input);
            for route in matches {
                eprintln!("  {}", route.name);
            }
            std::process::exit(1);
        }
    }
}

fn resolve_recovery_name(
    explicit: &Option<String>,
    default_name: &str,
    action: &str,
) -> Result<String> {
    if let Some(name) = explicit {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(default_name.to_string());
        }
        if !is_valid_route_name(trimmed) {
            eprintln!("[ERROR] Invalid route name '{}'.", trimmed);
            eprintln!("Allowed: letters, digits, '-', '_', '/'");
            std::process::exit(1);
        }
        return Ok(trimmed.to_string());
    }

    loop {
        eprint!(
            "[INPUT] Enter new route name to {} (empty for {}): ",
            action, default_name
        );
        io::stderr().flush().ok();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read input")?;
        let name = input.trim();
        if name.is_empty() {
            return Ok(default_name.to_string());
        }
        if is_valid_route_name(name) {
            return Ok(name.to_string());
        }
        eprintln!("Route name may contain letters, digits, '-', '_', '/'.");
    }
}

fn handle_recovery(
    save_dir: &Path,
    list: bool,
    name: &Option<String>,
    identifier: &Option<String>,
) -> Result<()> {
    let routes = list_recovery_routes(save_dir)?;
    if list || identifier.is_none() {
        if routes.is_empty() {
            println!("No recovery routes.");
            return Ok(());
        }
        println!("Recovery routes:");
        for route in routes {
            let detail = route
                .latest_save
                .as_ref()
                .map(|s| format!(" - {} ({})", s.message, s.short_id))
                .unwrap_or_default();
            println!("  {}{}", route.name, detail);
        }
        return Ok(());
    }

    let target_input = identifier.as_ref().unwrap();
    if routes.is_empty() {
        eprintln!("[ERROR] No recovery routes available.");
        std::process::exit(1);
    }
    let target = resolve_recovery_route(&routes, target_input)?;
    let short_hash = target.name.chars().take(7).collect::<String>();
    let default_name = format!("recovery-{}", short_hash);
    let new_name = resolve_recovery_name(name, &default_name, "recover discard")?;

    let all_routes = RouteManager::new(Git2Core::open(save_dir)?).list_routes()?;
    if all_routes.iter().any(|route| route.name == new_name) {
        eprintln!("[ERROR] Route '{}' already exists.", new_name);
        std::process::exit(1);
    }

    let mut guard_manager = SaveManager::new(Git2Core::open(save_dir)?);
    if !ensure_clean_for_action(&mut guard_manager, "switching to recovery route")? {
        println!("Cancelled.");
        return Ok(());
    }

    let mut core = Git2Core::open(save_dir)?;
    core.rename_route(&target.name, &new_name)
        .context("Failed to rename recovery route")?;
    core.switch_route(&new_name)
        .context("Failed to switch to recovery route")?;

    println!("[OK] Recovered to route: {}", new_name);
    Ok(())
}

fn handle_route(save_dir: &Path, list_flag: bool, command: &Option<RouteCommands>) -> Result<()> {
    let core = Git2Core::open(save_dir).context("Failed to open repository")?;
    let mut manager = RouteManager::new(core);

    if list_flag && command.is_none() {
        return print_routes(&manager);
    }

    match command {
        Some(RouteCommands::List) => {
            return print_routes(&manager);
        }
        Some(RouteCommands::Create { name }) => {
            let mut guard_manager = SaveManager::new(Git2Core::open(save_dir)?);
            if !ensure_clean_for_action(&mut guard_manager, "creating a route")? {
                println!("Cancelled.");
                return Ok(());
            }
            manager
                .create_route(name)
                .context("Failed to create route")?;
            println!("[OK] Created route: {}", name);
        }
        Some(RouteCommands::Switch { name, create }) => {
            let mut guard_manager = SaveManager::new(Git2Core::open(save_dir)?);
            if !ensure_clean_for_action(&mut guard_manager, "switching routes")? {
                println!("Cancelled.");
                return Ok(());
            }
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
                            eprintln!(
                                "No routes available. Use 'gitsave route create <name>' to create one."
                            );
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
        Some(RouteCommands::Rename { old_name, new_name }) => {
            let all_routes = manager.list_routes().context("Failed to list routes")?;
            let old_exists = all_routes.iter().any(|r| r.name == *old_name);

            if !old_exists {
                eprintln!("[ERROR] Route not found: {}", old_name);
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

            let new_exists = all_routes.iter().any(|r| r.name == *new_name);
            if new_exists {
                eprintln!("[ERROR] Route '{}' already exists", new_name);
                std::process::exit(1);
            }

            match manager.rename_route(old_name, new_name) {
                Ok(()) => println!("[OK] Renamed route: {} -> {}", old_name, new_name),
                Err(e) => {
                    eprintln!("[ERROR] Failed to rename route: {}", e);
                    std::process::exit(1);
                }
            }
        }
        None => {
            if list_flag {
                return print_routes(&manager);
            }
            let current_route = manager
                .get_current_route()
                .context("Failed to get current route")?;
            println!("Current route: {}", current_route);
            println!("  Use 'gitsave route --list' to see all routes");
        }
    }
    Ok(())
}

enum DirtyDecision {
    Save,
    Discard,
    Cancel,
}

enum UnstableDecision {
    Force,
    Retry,
    Cancel,
}

fn confirm_discard_changes(message: &str) -> Result<bool> {
    eprint!(
        "[WARN] {} A recovery snapshot will be created. [y/N]: ",
        message
    );
    io::stderr().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    let resp = input.trim().to_lowercase();
    Ok(resp == "y" || resp == "yes")
}

fn prompt_dirty_decision(action: &str) -> Result<DirtyDecision> {
    loop {
        eprint!(
            "[WARN] Uncommitted changes detected. {} requires a clean working tree. ",
            action
        );
        eprint!("Choose (s)ave, (d)iscard (with recovery), (c)ancel: ");
        io::stderr().flush().ok();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read input")?;
        match input.trim().to_lowercase().as_str() {
            "s" | "save" => return Ok(DirtyDecision::Save),
            "d" | "discard" => return Ok(DirtyDecision::Discard),
            "c" | "cancel" | "" => return Ok(DirtyDecision::Cancel),
            _ => eprintln!("Please enter s, d, or c."),
        }
    }
}

fn prompt_unstable_decision(attempts: u32) -> Result<UnstableDecision> {
    loop {
        eprint!(
            "[WARN] Save files still changing after {} checks. ",
            attempts
        );
        eprint!("Choose (f)orce, (r)etry, (c)ancel: ");
        io::stderr().flush().ok();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read input")?;
        match input.trim().to_lowercase().as_str() {
            "f" | "force" => return Ok(UnstableDecision::Force),
            "r" | "retry" => return Ok(UnstableDecision::Retry),
            "c" | "cancel" | "" => return Ok(UnstableDecision::Cancel),
            _ => eprintln!("Please enter f, r, or c."),
        }
    }
}

fn is_valid_route_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/'))
}

fn resolve_route_name(route: &Option<String>, action: &str) -> Result<Option<String>> {
    if let Some(name) = route {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            eprintln!("[ERROR] Route name cannot be empty.");
            std::process::exit(1);
        }
        if !is_valid_route_name(trimmed) {
            eprintln!("[ERROR] Invalid route name '{}'.", trimmed);
            eprintln!("Allowed: letters, digits, '-', '_', '/'");
            std::process::exit(1);
        }
        return Ok(Some(trimmed.to_string()));
    }

    loop {
        eprint!(
            "[INPUT] Enter new route name to {} (empty to cancel): ",
            action
        );
        io::stderr().flush().ok();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read input")?;
        let name = input.trim();
        if name.is_empty() {
            return Ok(None);
        }
        if is_valid_route_name(name) {
            return Ok(Some(name.to_string()));
        }
        eprintln!("Route name may contain letters, digits, '-', '_', '/'.");
    }
}

fn perform_stable_save_interactive(
    manager: &mut SaveManager,
    message: &str,
) -> Result<Option<core::SaveResult>> {
    loop {
        match manager.save(message) {
            Ok(result) => return Ok(Some(result)),
            Err(SaveError::UnstableSave { attempts }) => match prompt_unstable_decision(attempts)? {
                UnstableDecision::Force => {
                    let result = manager.save_force(message)?;
                    return Ok(Some(result));
                }
                UnstableDecision::Retry => continue,
                UnstableDecision::Cancel => return Ok(None),
            },
            Err(err) => return Err(err).context("Failed to save"),
        }
    }
}

fn ensure_clean_for_action(manager: &mut SaveManager, action: &str) -> Result<bool> {
    let status = manager.get_status()?;
    if !status.has_uncommitted_changes {
        return Ok(true);
    }
    match prompt_dirty_decision(action)? {
        DirtyDecision::Save => {
            let message = format!("[guard] before {}", action);
            let result = perform_stable_save_interactive(manager, &message)?;
            if let Some(result) = result {
                manager.update_last_save_time();
                println!(
                    "[OK] Saved changes before {} ({}).",
                    action, result.short_oid
                );
                Ok(true)
            } else {
                Ok(false)
            }
        }
        DirtyDecision::Discard => {
            manager.discard_changes()?;
            println!("[OK] Discarded uncommitted changes.");
            Ok(true)
        }
        DirtyDecision::Cancel => Ok(false),
    }
}

fn print_routes(manager: &RouteManager) -> Result<()> {
    let mut routes = manager.list_routes().context("Failed to list routes")?;
    routes.retain(|route| !is_recovery_branch_name(&route.name));
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

    if cli.command.is_none() {
        #[cfg(feature = "gui")]
        {
            if let Err(e) = gui::run(&save_dir) {
                #[cfg(windows)]
                {
                    let msg = format!("Gitsave GUI error: {e}");
                    let log_path = std::env::temp_dir().join("gitsave_crash.log");
                    let log_written = std::fs::write(&log_path, &msg).is_ok();
                    unsafe {
                        use std::ffi::OsStr;
                        use std::os::windows::ffi::OsStrExt;
                        let body_str = if log_written {
                            format!(
                                "{msg}\n\nA full log has been written to:\n{}",
                                log_path.display()
                            )
                        } else {
                            msg
                        };
                        let mut title: Vec<u16> = OsStr::new("Gitsave \u{2014} Fatal Error")
                            .encode_wide().collect();
                        title.push(0);
                        let mut body: Vec<u16> = OsStr::new(&body_str)
                            .encode_wide().collect();
                        body.push(0);
                        windows_message_box(title.as_ptr(), body.as_ptr());
                    }
                }
                #[cfg(not(windows))]
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(not(feature = "gui"))]
        {
            let _ = Cli::command().print_help();
            println!();
            std::process::exit(2);
        }
    }

    match cli.command.as_ref().expect("command present") {
        Commands::Init { path, force } => {
            if let Err(e) = handle_init(&path, *force) {
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
        Commands::Amend { message, desc } => {
            let msg = message.clone().unwrap_or_else(|| desc.clone());
            if let Err(e) = handle_amend(&save_dir, &msg) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Load {
            list,
            preview,
            force,
            tag,
            route,
            identifier,
        } => {
            if let Err(e) =
                handle_load(&save_dir, *list, *preview, *force, tag, route, identifier)
            {
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
        Commands::Route { list, command } => {
            if let Err(e) = handle_route(&save_dir, *list, command) {
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
            delete,
            name,
            message,
        } => {
            if *delete {
                if let Some(tag_name) = name {
                    match Git2Core::open(&save_dir) {
                        Ok(core) => {
                            let manager = SaveManager::new(core);
                            if let Err(e) = manager.delete_tag(&tag_name) {
                                eprintln!("Error: Failed to delete tag: {}", e);
                                std::process::exit(1);
                            }
                            println!("Deleted tag: {}", tag_name);
                        }
                        Err(e) => {
                            eprintln!("Error: Failed to open repository: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: Tag name required for --delete");
                    std::process::exit(1);
                }
                return;
            }

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
        Commands::Autosave {
            enable,
            interval,
            max_count,
            status,
            disable,
        } => {
            handle_autosave(&save_dir, *enable, *interval, *max_count, *status, *disable);
        }
        Commands::Tui => {
            if let Err(e) = tui::run(&save_dir) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        #[cfg(feature = "gui")]
        Commands::Gui => {
            if let Err(e) = gui::run(&save_dir) {
                // On Windows GUI builds the console is hidden, so eprintln! is
                // silently discarded.  Write a crash log that the user can find.
                #[cfg(windows)]
                {
                    let msg = format!("Gitsave GUI error: {e}");
                    let log_path = std::env::temp_dir().join("gitsave_crash.log");
                    let log_written = std::fs::write(&log_path, &msg).is_ok();
                    // Also show a message box so the error is immediately visible.
                    unsafe {
                        use std::ffi::OsStr;
                        use std::os::windows::ffi::OsStrExt;
                        let body_str = if log_written {
                            format!(
                                "{msg}\n\nA full log has been written to:\n{}",
                                log_path.display()
                            )
                        } else {
                            msg
                        };
                        let mut title: Vec<u16> = OsStr::new("Gitsave \u{2014} Fatal Error")
                            .encode_wide().collect();
                        title.push(0);
                        let mut body: Vec<u16> = OsStr::new(&body_str)
                            .encode_wide().collect();
                        body.push(0);
                        windows_message_box(title.as_ptr(), body.as_ptr());
                    }
                }
                #[cfg(not(windows))]
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Recovery {
            list,
            name,
            identifier,
        } => {
            if let Err(e) = handle_recovery(&save_dir, *list, name, identifier) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
