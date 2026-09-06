//! Kelp - A fast, queryable, object-oriented embedded database
//!
//! Modern CLI for database operations and interactive development.

use kelp_db::{Database, Shell, ShellConfig};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const VERSION: &str = "0.2.0";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_main_help();
        return;
    }

    match args[0].as_str() {
        "create" => handle_create(&args[1..]),
        "delete" => handle_delete(&args[1..]),
        "inspect" => handle_inspect(&args[1..]),
        "shell" => handle_shell(&args[1..]),
        "help" | "--help" | "-h" => handle_help(&args[1..]),
        "--version" | "-v" => println!("Kelp {}", VERSION),
        _ => {
            eprintln!("Unknown command: {}", args[0]);
            print_main_help();
            std::process::exit(1);
        }
    }
}

/// Create a new database at the given path
fn handle_create(args: &[String]) {
    if args.is_empty() {
        println!("Usage: kelp create <path> [--name <database_name>]");
        println!("\nExamples:");
        println!("  kelp create ./my_database");
        println!("  kelp create /tmp/kelp_db --name production");
        return;
    }

    let path = Path::new(&args[0]);
    let mut db_name = None;

    // Parse optional --name flag
    for i in 0..args.len() - 1 {
        if args[i] == "--name" {
            db_name = Some(args[i + 1].clone());
        }
    }

    if path.exists() {
        eprintln!("Database already exists at: {}", path.display());
        return;
    }

    match fs::create_dir_all(path) {
        Ok(_) => {
            println!("✓ Creating database at: {}", path.display());
            match Database::open_local(path) {
                Ok(db) => {
                    // Set custom name if provided
                    if let Some(name) = db_name {
                        if let Err(e) = db.set_name(&name) {
                            eprintln!("Warning: Could not set database name: {}", e);
                        } else {
                            println!("✓ Database name set to: {}", name);
                        }
                    }

                    println!("✓ Database initialized successfully!");
                    println!("\nNext steps:");
                    println!("  kelp shell {} # Launch the interactive shell", path.display());
                    println!("  kelp inspect {} # View database details", path.display());
                }
                Err(e) => {
                    eprintln!("✗ Failed to initialize database: {}", e);
                    let _ = fs::remove_dir_all(path);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to create directory: {}", e);
        }
    }
}

/// Delete a database at the given path
fn handle_delete(args: &[String]) {
    if args.is_empty() {
        println!("Usage: kelp delete <path>");
        println!("\nExamples:");
        println!("  kelp delete ./my_database");
        println!("  kelp delete /tmp/kelp_db");
        return;
    }

    let path = Path::new(&args[0]);

    let metadata_path = path.join(".kelp");
    if !metadata_path.exists() {
        eprintln!("Database not found at: {}", path.display());
        return;
    }

    println!("⚠️  This will permanently delete the database at:");
    println!("  {}", path.display());
    print!("\nAre you sure? [y/N]: ");
    io::stdout().flush().ok();

    let mut response = String::new();
    if io::stdin().read_line(&mut response).is_err() {
        return;
    }

    if response.trim().eq_ignore_ascii_case("y") || response.trim().eq_ignore_ascii_case("yes") {
        match fs::remove_dir_all(path) {
            Ok(_) => println!("✓ Database deleted successfully"),
            Err(e) => eprintln!("✗ Failed to delete database: {}", e),
        }
    } else {
        println!("Cancelled.");
    }
}

/// Inspect and display database information
fn handle_inspect(args: &[String]) {
    let path = if args.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&args[0])
    };

    let metadata_path = path.join(".kelp");
    if !metadata_path.exists() {
        eprintln!("Database not found at: {}", path.display());
        return;
    }

    match Database::open_local(&path) {
        Ok(db) => {
            match db.inspect() {
                Ok(summary) => {
                    println!("\n╔════════════════════════════════════════════════════════╗");
                    println!("║              Database Inspection Report                  ║");
                    println!("╚════════════════════════════════════════════════════════╝\n");

                    println!("📦 Database Information:");
                    println!("   Name:              {}", summary.name);
                    println!("   Path:              {}", summary.path);
                    println!("   Backend:           {}\n", summary.storage_backend);

                    println!("📊 Statistics:");
                    println!("   Total Size:        {} bytes", format_bytes(summary.total_size_bytes));
                    println!("   Schemas:           {}", summary.schema_count);
                    println!("   Object Types:      {}", summary.object_type_count);
                    println!("   Objects:           {}\n", summary.object_count);

                    if !summary.precomputed_queries.is_empty() {
                        println!("⚡ Precomputed Queries:");
                        for q in summary.precomputed_queries {
                            println!("   • {}", q);
                        }
                        println!();
                    }

                    println!("💡 Quick commands:");
                    println!("   kelp shell {}  # Launch interactive shell", path.display());
                    println!("   kelp delete {} # Delete this database", path.display());
                    println!();
                }
                Err(e) => eprintln!("✗ Error inspecting database: {}", e),
            }
        }
        Err(e) => eprintln!("✗ Failed to open database: {}", e),
    }
}

/// Launch the interactive shell
fn handle_shell(args: &[String]) {
    let path = if args.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&args[0])
    };

    if !path.exists() {
        eprintln!("Database not found at: {}", path.display());
        eprintln!("\nCreate a database first:");
        eprintln!("  kelp create {}", path.display());
        return;
    }

    // Check if database has been initialized (must have .kelp metadata directory)
    let metadata_path = path.join(".kelp");
    if !metadata_path.exists() {
        eprintln!("Database not found at: {}", path.display());
        eprintln!("\nCreate a database first:");
        eprintln!("  kelp create {}", path.display());
        return;
    }

    // Parse --debug flag
    let debug = args.iter().any(|a| a == "--debug");

    match Database::open_local(&path) {
        Ok(db) => {
            let config = ShellConfig {
                debug,
                show_timing: debug,
                track_storage: debug,
            };

            let mut shell = Shell::new(db, config);
            shell.run();
        }
        Err(e) => {
            eprintln!("✗ Failed to open database: {}", e);
        }
    }
}

/// Print help information
fn handle_help(args: &[String]) {
    if args.is_empty() {
        print_main_help();
    } else {
        match args[0].as_str() {
            "create" => {
                println!("\n╔════════════════════════════════════════════════════════╗");
                println!("║                 kelp create - Create DB                 ║");
                println!("╚════════════════════════════════════════════════════════╝\n");
                println!("Create a new Kelp database at a given path.\n");
                println!("USAGE:");
                println!("  kelp create <path> [--name <database_name>]\n");
                println!("ARGUMENTS:");
                println!("  <path>                 Directory path for the database\n");
                println!("OPTIONS:");
                println!("  --name <name>         Assign a custom name to the database\n");
                println!("EXAMPLES:");
                println!("  kelp create ./my_db");
                println!("  kelp create /tmp/prod_db --name production");
                println!("  kelp create ~/data/users_db\n");
            }
            "delete" => {
                println!("\n╔════════════════════════════════════════════════════════╗");
                println!("║               kelp delete - Delete DB                   ║");
                println!("╚════════════════════════════════════════════════════════╝\n");
                println!("Permanently delete a database and all its data.\n");
                println!("USAGE:");
                println!("  kelp delete <path>\n");
                println!("ARGUMENTS:");
                println!("  <path>                 Path to the database to delete\n");
                println!("WARNING: This action is permanent and cannot be undone!\n");
                println!("EXAMPLES:");
                println!("  kelp delete ./my_db");
                println!("  kelp delete /tmp/old_database\n");
            }
            "inspect" => {
                println!("\n╔════════════════════════════════════════════════════════╗");
                println!("║               kelp inspect - View DB Info               ║");
                println!("╚════════════════════════════════════════════════════════╝\n");
                println!("Display detailed information about a database.\n");
                println!("USAGE:");
                println!("  kelp inspect [<path>]\n");
                println!("ARGUMENTS:");
                println!("  <path>                 Path to the database (default: current dir)\n");
                println!("EXAMPLES:");
                println!("  kelp inspect");
                println!("  kelp inspect ./my_db");
                println!("  kelp inspect /tmp/prod_db\n");
            }
            "shell" => {
                println!("\n╔════════════════════════════════════════════════════════╗");
                println!("║              kelp shell - Interactive Shell             ║");
                println!("╚════════════════════════════════════════════════════════╝\n");
                println!("Launch an interactive REPL for database operations.\n");
                println!("USAGE:");
                println!("  kelp shell [<path>] [--debug]\n");
                println!("ARGUMENTS:");
                println!("  <path>                 Path to the database (default: current dir)\n");
                println!("OPTIONS:");
                println!("  --debug               Enable performance debugging output\n");
                println!("EXAMPLES:");
                println!("  kelp shell");
                println!("  kelp shell ./my_db");
                println!("  kelp shell . --debug\n");
                println!("SHELL COMMANDS:");
                println!("  schema create|list|show");
                println!("  object create|get|list|update|delete");
                println!("  query <Type> <conditions>");
                println!("  inspect");
                println!("  debug [on|off|status]");
                println!("  help [command]");
                println!("  exit\n");
            }
            _ => {
                println!("No help available for '{}'", args[0]);
                print_main_help();
            }
        }
    }
}

fn print_main_help() {
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║    Kelp Database - Fast, Queryable, Object-Oriented    ║");
    println!("║                   v{:<37}║", VERSION);
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("COMMANDS:\n");

    println!("  create <path> [--name <name>]");
    println!("      Create a new database at <path>\n");

    println!("  shell [<path>] [--debug]");
    println!("      Launch interactive shell (default path: .)\n");

    println!("  inspect [<path>]");
    println!("      Show database statistics and info (default path: .)\n");

    println!("  delete <path>");
    println!("      Permanently delete a database\n");

    println!("  help [<command>]");
    println!("      Show help for a command with examples\n");

    println!("EXAMPLES:\n");

    println!("  1. Create a new database:");
    println!("     $ kelp create ./my_app_db --name \"My App\"");
    println!("     ✓ Database initialized successfully!\n");

    println!("  2. Launch the interactive shell:");
    println!("     $ kelp shell ./my_app_db");
    println!("     kelp> schema create User name:string:required email:string:required");
    println!("     kelp> object create User u1 name=Alice email=alice@example.com");
    println!("     kelp> query User name=Alice");
    println!("     kelp> exit\n");

    println!("  3. Inspect database details:");
    println!("     $ kelp inspect ./my_app_db");
    println!("     📦 Database Information:");
    println!("        Name:  My App");
    println!("        Path:  ./my_app_db\n");

    println!("  4. Get help for a command:");
    println!("     $ kelp help shell");
    println!("     $ kelp help create\n");

    println!("OPTIONS:\n");
    println!("  --version, -v          Show version");
    println!("  --help, -h             Show this help message\n");

    println!("For more information and tutorials, visit:");
    println!("  https://github.com/OneAM-Labs/kelp\n");
}



fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}
