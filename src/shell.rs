//! Interactive Kelp database shell (REPL).
//!
//! The shell provides an interactive command-line interface for:
//! - Defining object types (schemas)
//! - Creating, reading, updating, and deleting objects
//! - Querying objects
//! - Performance metrics and debugging

use crate::query::Predicate;
use crate::schema::FieldDef;
use crate::{Database, FieldType, Object, Schema, Value};
use crate::storage::page::{PageType, PageId};
use crate::storage::page_manager::{FilePageStorage, PageManager};
use crate::storage::slotted_page::SlottedPageLayout;
use std::io::{self, Write};
use std::time::Instant;

/// Configuration for the shell session.
pub struct ShellConfig {
    /// Enable performance debugging output
    pub debug: bool,
    /// Show timing information for commands
    pub show_timing: bool,
    /// Track storage changes
    pub track_storage: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            debug: false,
            show_timing: false,
            track_storage: false,
        }
    }
}

/// The interactive shell session.
pub struct Shell {
    db: Database,
    config: ShellConfig,
    history: Vec<String>,
}

impl Shell {
    /// Create a new shell session for a database.
    pub fn new(db: Database, config: ShellConfig) -> Self {
        Self {
            db,
            config,
            history: Vec::new(),
        }
    }

    /// Run the interactive shell.
    pub fn run(&mut self) {
        println!("╔════════════════════════════════════════════════════════╗");
        println!("║          Kelp Database Shell v0.2                      ║");
        println!("╚════════════════════════════════════════════════════════╝");

        // Display database info
        if let Ok(summary) = self.db.inspect() {
            println!("\nDatabase: {}", summary.name);
            println!("Path:     {}", summary.path);
            println!("Size:     {} bytes", summary.total_size_bytes);
            println!();
        }

        if self.config.debug {
            println!("[DEBUG MODE ENABLED]");
            println!();
        }

        println!("Type 'help' for commands. Type 'exit' to quit.\n");

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            print!("kelp> ");
            let _ = stdout.flush();

            let mut line = String::new();
            if stdin.read_line(&mut line).is_err() {
                break;
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            self.history.push(line.to_string());

            if line == "exit" || line == "quit" {
                println!("Goodbye!");
                break;
            }

            let start_time = Instant::now();
            let initial_storage = if self.config.track_storage {
                self.db.inspect().ok().map(|s| s.total_size_bytes)
            } else {
                None
            };

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "server" => self.handle_server_command(&parts[1..]),
                "help" => self.print_help(&parts[1..]),
                "schema" => self.handle_schema_command(&parts[1..]),
                "object" => self.handle_object_command(&parts[1..]),
                "query" => self.handle_query_command(&parts[1..]),
                "inspect" => self.handle_inspect_command(&parts[1..]),
                "list" => self.handle_list_command(&parts[1..]),
                "debug" => self.handle_debug_command(&parts[1..]),
                "precompute" => self.handle_precompute_command(&parts[1..]),
                "clear" => println!("(Not yet implemented: clearing objects)"),
                _ => println!(
                    "Unknown command '{}'. Type 'help' for available commands.",
                    parts[0]
                ),
            }

            if self.config.show_timing {
                let elapsed = start_time.elapsed();
                println!(
                    "\n⏱  Execution time: {:.2}ms",
                    elapsed.as_secs_f64() * 1000.0
                );
            }

            if let Some(initial) = initial_storage {
                if let Ok(summary) = self.db.inspect() {
                    let diff = summary.total_size_bytes as i64 - initial as i64;
                    if diff != 0 {
                        let sign = if diff > 0 { "+" } else { "" };
                        println!("📦 Storage change: {}{} bytes", sign, diff);
                    }
                }
            }

            println!();
        }
    }

    fn print_help(&self, args: &[&str]) {
        if args.is_empty() {
            println!("\n╔════════════════════════════════════════════════════════╗");
            println!("║                  Available Commands                     ║");
            println!("╚════════════════════════════════════════════════════════╝\n");

            println!("SCHEMA MANAGEMENT:");
            println!("  schema create <Name> [field:type[:required|nullable]]");
            println!("    └─ Create a new object type");
            println!("  schema list");
            println!("    └─ List all schemas");
            println!("  schema show <Name>");
            println!("    └─ Display schema details\n");

            println!("OBJECT OPERATIONS:");
            println!("  object create <Type> <Id> [field=value ...]");
            println!("    └─ Create a new object");
            println!("  object get <Type> <Id>");
            println!("    └─ Get a specific object");
            println!("  object list <Type> [--limit N]");
            println!("    └─ List objects of a type");
            println!("  object update <Type> <Id> field=value [...]");
            println!("    └─ Update an object");
            println!("  object delete <Type> <Id>");
            println!("    └─ Delete an object\n");

            println!("QUERYING:");
            println!("  query <Type> field=value [field=value ...]");
            println!("    └─ Query by exact match");
            println!("  query <Type> field>value");
            println!("    └─ Query with comparisons (>, <, >=, <=, !=)\n");

            println!("DATABASE:");
            println!("  inspect");
            println!("    └─ Show database stats");
            println!("  list <Type>");
            println!("    └─ List objects of type\n");

            println!("DEBUG & CONFIG:");
            println!("  debug [on|off|true|false|status]");
            println!("    └─ Toggle debug mode (shows timing/storage changes)");
            println!("  precompute save <name> <query>");
            println!("    └─ Save a query for reuse\n");

            println!("OTHER:");
            println!("  help [command]");
            println!("    └─ Show this message or help for a specific command");
            println!("  exit / quit");
            println!("    └─ Exit the shell\n");
            println!("SERVER:");
            println!("  server start [addr] [--auth-token <token>]");
            println!("    └─ Start the HTTP API server bound to this database");
        } else if let Some(cmd) = args.get(0) {
            match *cmd {
                "schema" => {
                    println!("\nSCHEMA COMMANDS:");
                    println!("  schema create <Name> [field:type[:required|nullable]]");
                    println!("    Creates a new object type with optional field definitions.");
                    println!("    Example:  schema create User name:string:required email:string:required age:integer\n");
                    println!("  schema list");
                    println!("    Lists all defined object types.\n");
                    println!("  schema show <Name>");
                    println!("    Displays the fields and properties of a type.\n");
                }
                "object" => {
                    println!("\nOBJECT COMMANDS:");
                    println!("  object create <Type> <Id> [field=value ...]");
                    println!("    Example:  object create User u1 name=Alice age=30\n");
                    println!("  object get <Type> <Id>");
                    println!("    Fetch a specific object.\n");
                    println!("  object list <Type> [--limit N]");
                    println!("    List all objects of a type.\n");
                    println!("  object delete <Type> <Id>");
                    println!("    Remove an object.\n");
                }
                "query" => {
                    println!("\nQUERY COMMANDS:");
                    println!("  query <Type> field=value");
                    println!("    Find objects where field equals value.\n");
                    println!("  query <Type> field>value");
                    println!("    Support operators: >, <, >=, <=, !=\n");
                }
                "server" => {
                    println!("\nSERVER COMMANDS:");
                    println!("  server start [addr] [--auth-token <token>]");
                    println!("    Start the HTTP API server for the current database path.");
                    println!("    Example: server start 0.0.0.0:7878 --auth-token mytoken");
                    println!();
                }
                _ => println!("No additional help for '{}'", cmd),
            }
        }
    }

    fn handle_schema_command(&mut self, args: &[&str]) {
        if args.is_empty() {
            println!("Usage: schema create|list|show");
            return;
        }

        match args[0] {
            "create" => {
                if args.len() < 2 {
                    println!("Usage: schema create <Name> [field:type[:required|nullable]]");
                    return;
                }

                let type_name = args[1];
                let mut schema = Schema::new(type_name);

                for spec in &args[2..] {
                    let field = parse_field_spec(spec);
                    schema = schema.add_field(field);
                }

                match self.db.create_type(&schema) {
                    Ok(_) => println!("✓ Schema '{}' created", type_name),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            "list" => match self.db.list_types() {
                Ok(types) => {
                    if types.is_empty() {
                        println!("No schemas defined.");
                    } else {
                        println!("\nDefined schemas:");
                        for name in types {
                            println!("  - {}", name);
                        }
                        println!();
                    }
                }
                Err(e) => println!("✗ Error: {}", e),
            },

            "show" => {
                if args.len() < 2 {
                    println!("Usage: schema show <Name>");
                    return;
                }

                match self.db.get_type(args[1]) {
                    Ok(Some(schema)) => {
                        println!("\nSchema: {}", schema.name);
                        println!("  Fields:");
                        for field_name in schema.field_names() {
                            if let Some(field) = schema.get_field(field_name) {
                                let mut flags = Vec::new();
                                if field.required {
                                    flags.push("required");
                                }
                                if field.nullable {
                                    flags.push("nullable");
                                }
                                let flags_str = if flags.is_empty() {
                                    String::new()
                                } else {
                                    format!(" ({})", flags.join(", "))
                                };
                                println!(
                                    "    • {} : {}{}",
                                    field.name,
                                    field.field_type.as_str(),
                                    flags_str
                                );
                            }
                        }
                        println!();
                    }
                    Ok(None) => println!("Schema '{}' not found.", args[1]),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            _ => println!("Unknown schema command. Use: create, list, or show"),
        }
    }

    fn handle_object_command(&mut self, args: &[&str]) {
        if args.is_empty() {
            println!("Usage: object create|get|list|update|delete");
            return;
        }

        match args[0] {
            "create" => {
                if args.len() < 3 {
                    println!("Usage: object create <Type> <Id> [field=value ...]");
                    return;
                }

                let type_name = args[1];
                let object_id = args[2];
                let mut obj = Object::new(type_name, object_id);

                for assignment in &args[3..] {
                    if let Some((key, value)) = assignment.split_once('=') {
                        obj.set_field(key, parse_value(value));
                    } else {
                        println!(
                            "Invalid field assignment: '{}'. Use field=value",
                            assignment
                        );
                        return;
                    }
                }

                match self.db.create_object(&obj) {
                    Ok(_) => println!("✓ Created {}/{}", type_name, object_id),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            "get" => {
                if args.len() < 3 {
                    println!("Usage: object get <Type> <Id>");
                    return;
                }

                match self.db.get_object(args[1], args[2]) {
                    Ok(Some(obj)) => {
                        println!("\nObject: {}", obj.id);
                        for (field, value) in &obj.fields {
                            println!("  {} = {}", field, value);
                        }
                        println!();
                    }
                    Ok(None) => println!("Object {}/{} not found.", args[1], args[2]),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            "list" => {
                if args.len() < 2 {
                    println!("Usage: object list <Type> [--limit N]");
                    return;
                }

                let type_name = args[1];
                let limit: Option<usize> =
                    if let Some(idx) = args.iter().position(|&a| a == "--limit") {
                        args.get(idx + 1).and_then(|l| l.parse().ok())
                    } else {
                        None
                    };

                match self.db.list_objects(type_name) {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No objects of type '{}' found.", type_name);
                        } else {
                            println!(
                                "\nObjects of type '{}' ({} total):",
                                type_name,
                                results.len()
                            );
                            for (idx, obj) in results.iter().enumerate() {
                                if let Some(lim) = limit {
                                    if idx >= lim {
                                        println!("  ... and {} more", results.len() - idx);
                                        break;
                                    }
                                }
                                println!("  • {}", obj.id);
                            }
                            println!();
                        }
                    }
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            "delete" => {
                if args.len() < 3 {
                    println!("Usage: object delete <Type> <Id>");
                    return;
                }

                match self.db.delete_object(args[1], args[2]) {
                    Ok(_) => println!("✓ Deleted {}/{}", args[1], args[2]),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            "update" => {
                if args.len() < 3 {
                    println!("Usage: object update <Type> <Id> field=value [...]");
                    return;
                }

                let type_name = args[1];
                let object_id = args[2];

                match self.db.get_object(type_name, object_id) {
                    Ok(Some(mut obj)) => {
                        for assignment in &args[3..] {
                            if let Some((key, value)) = assignment.split_once('=') {
                                obj.set_field(key, parse_value(value));
                            }
                        }

                        match self.db.update_object(&obj) {
                            Ok(_) => println!("✓ Updated {}/{}", type_name, object_id),
                            Err(e) => println!("✗ Error: {}", e),
                        }
                    }
                    Ok(None) => println!("Object {}/{} not found.", type_name, object_id),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }

            _ => println!("Unknown object command. Use: create, get, list, update, or delete"),
        }
    }

    fn handle_query_command(&self, args: &[&str]) {
        if args.is_empty() {
            println!("Usage: query <Type> [field=value|field>value|...]");
            return;
        }

        let type_name = args[0];
        let mut predicates = Vec::new();

        for condition in &args[1..] {
            if let Some(pred) = parse_query_condition(condition) {
                predicates.push(pred);
            } else {
                println!("Invalid condition: {}", condition);
                return;
            }
        }

        if predicates.is_empty() {
            println!("No valid query conditions provided.");
            return;
        }

        let combined_predicate = if predicates.len() == 1 {
            predicates.into_iter().next().unwrap()
        } else {
            Predicate::and(predicates)
        };

        match self.db.query(type_name, &combined_predicate) {
            Ok(results) => {
                if results.objects.is_empty() {
                    println!("No results found.");
                } else {
                    println!("\nQuery results ({} found):", results.objects.len());
                    for obj in &results.objects {
                        println!("  • {}", obj.id);
                    }
                    println!();
                }
            }
            Err(e) => println!("✗ Query error: {}", e),
        }
    }

    fn handle_list_command(&self, args: &[&str]) {
        if args.is_empty() {
            println!("Usage: list <Type>");
            return;
        }

        match self.db.list_objects(args[0]) {
            Ok(results) => {
                if results.objects.is_empty() {
                    println!("No objects of type '{}'", args[0]);
                } else {
                    for obj in &results.objects {
                        println!("  {}", obj.id);
                    }
                }
            }
            Err(e) => println!("✗ Error: {}", e),
        }
    }

    fn handle_inspect_command(&self, _args: &[&str]) {
        match self.db.inspect() {
            Ok(summary) => {
                println!("\n╔════════════════════════════════════════════════════════╗");
                println!("║            Database Inspection Report                   ║");
                println!("╚════════════════════════════════════════════════════════╝");
                println!("\nDatabase Name:       {}", summary.name);
                println!("Path:                {}", summary.path);
                println!("Storage Backend:     {}", summary.storage_backend);
                println!("\nStatistics:");
                println!("  Total Size:        {} bytes", summary.total_size_bytes);
                println!("  Schemas:           {}", summary.schema_count);
                println!("  Object Types:      {}", summary.object_type_count);
                println!("  Objects:           {}", summary.object_count);

                if !summary.precomputed_queries.is_empty() {
                    println!("\nPrecomputed Queries:");
                    for query_name in summary.precomputed_queries {
                        println!("  • {}", query_name);
                    }
                }
                println!();
            }
            Err(e) => println!("✗ Error: {}", e),
        }
    }

    fn handle_debug_command(&mut self, args: &[&str]) {
        if args.is_empty() || args[0] == "status" {
            println!("\nDebug Status:");
            println!(
                "  Debug Mode:        {}",
                if self.config.debug { "ON" } else { "OFF" }
            );
            println!(
                "  Show Timing:       {}",
                if self.config.show_timing { "ON" } else { "OFF" }
            );
            println!(
                "  Track Storage:     {}",
                if self.config.track_storage {
                    "ON"
                } else {
                    "OFF"
                }
            );
            println!();
        } else if args[0] == "on"  || args[0] == "true" {
            self.config.debug = true;
            self.config.show_timing = true;
            self.config.track_storage = true;
            println!("✓ Debug mode enabled (timing and storage tracking ON)");
        } else if args[0] == "off" || args[0] == "false"  {
            self.config.debug = false;
            self.config.show_timing = false;
            self.config.track_storage = false;
            println!("✓ Debug mode disabled");
        } else if args[0] == "page" {
            // debug page <action> [...]
            if args.len() < 2 {
                println!("Usage: debug page list|read|write|allocate|info <args>");
                return;
            }

            // refuse pages commands for in-memory DBs
            let db_path = self.db.path();
            if db_path.to_string_lossy().starts_with("memory://") {
                println!("Page commands require a file-backed database");
                return;
            }

            let pages_file = db_path.join(".kelp").join("pages.db");
            match FilePageStorage::new(&pages_file) {
                Ok(mut fps) => {
                    let mut pm = PageManager::new(Box::new(fps));
                    match args[1] {
                        "list" => {
                            match pm.size() {
                                Ok(len) => {
                                    let count = (len / crate::storage::page::PAGE_SIZE as u64) as u32;
                                    println!("Pages in file: {} (including superblock)", count);
                                    if count <= 1 {
                                        println!("No user pages allocated.");
                                    } else {
                                        for id in 1..count {
                                            println!("  - Page {} (Type: Object)", id);
                                        }
                                    }
                                }
                                Err(e) => println!("✗ Error: {}", e),
                            }
                        }

                        "allocate" => {
                            if args.len() < 3 {
                                println!("Usage: debug page allocate <Object|Index|FreeSpace|Overflow>");
                                return;
                            }
                            let t = match args[2].to_lowercase().as_str() {
                                "object" => PageType::Object,
                                "index" => PageType::Index,
                                "overflow" => PageType::Overflow,
                                "freespace" | "free" => PageType::FreeSpace,
                                _ => {
                                    println!("Unknown page type: {}", args[2]);
                                    return;
                                }
                            };

                            match pm.allocate_page(t) {
                                Ok(pid) => {
                                    println!("Allocated page {}", pid.as_u32());
                                    println!("Type: {:?}", t);
                                    println!("Size: {} bytes", crate::storage::page::PAGE_SIZE);
                                }
                                Err(e) => println!("✗ Error: {}", e),
                            }
                        }

                        "read" => {
                            if args.len() < 3 {
                                println!("Usage: debug page read <page_id>");
                                return;
                            }
                            if let Ok(n) = args[2].parse::<u32>() {
                                let pid = PageId::new(n);
                                match pm.read_page(pid) {
                                    Ok(page) => {
                                        println!("Page {}", page.page_id.as_u32());
                                        println!("Type: {:?}", page.page_type);
                                        println!("Size: {} bytes", page.data.len());
                                        // show a short preview of data
                                        let preview_len = 128.min(page.data.len());
                                        let snippet = &page.data[..preview_len];
                                        if let Ok(s) = std::str::from_utf8(snippet) {
                                            println!("Data (first {} bytes):\n{}", preview_len, s);
                                        } else {
                                            // hex preview
                                            for (i, chunk) in snippet.chunks(16).enumerate() {
                                                print!("{:08x}  ", i * 16);
                                                for b in chunk {
                                                    print!("{:02x}", b);
                                                }
                                                println!();
                                            }
                                        }
                                    }
                                    Err(e) => println!("✗ Error: {}", e),
                                }
                            } else {
                                println!("Invalid page id: {}", args[2]);
                            }
                        }

                        "write" => {
                            if args.len() < 4 {
                                println!("Usage: debug page write <page_id> <text>");
                                return;
                            }
                            if let Ok(n) = args[2].parse::<u32>() {
                                let pid = PageId::new(n);
                                match pm.read_page(pid) {
                                    Ok(mut page) => {
                                        let text = args[3..].join(" ");
                                        let bytes = text.as_bytes();
                                        let copy_len = bytes.len().min(page.data.len());
                                        page.data[..copy_len].copy_from_slice(&bytes[..copy_len]);
                                        if let Err(e) = pm.write_page(&page) {
                                            println!("✗ Error writing page: {}", e);
                                        } else if let Err(e) = pm.sync() {
                                            println!("✗ Error syncing: {}", e);
                                        } else {
                                            println!("Wrote {} bytes to page {}", copy_len, pid.as_u32());
                                        }
                                    }
                                    Err(e) => println!("✗ Error: {}", e),
                                }
                            } else {
                                println!("Invalid page id: {}", args[2]);
                            }
                        }

                        "info" => {
                            if args.len() < 3 {
                                println!("Usage: debug page info <page_id>");
                                return;
                            }
                            if let Ok(n) = args[2].parse::<u32>() {
                                let pid = PageId::new(n);
                                match pm.read_page(pid) {
                                    Ok(page) => {
                                        println!("Page ID: {}", page.page_id.as_u32());
                                        println!("Type: {:?}", page.page_type);
                                        println!("Page size: {} bytes", page.data.len());
                                        // slotted page info
                                        if let Ok(layout) = SlottedPageLayout::from_page(&page) {
                                            println!("Slot count: {}", layout.slot_count());
                                            println!("Free bytes: {}", layout.free_space());
                                        }
                                    }
                                    Err(e) => println!("✗ Error: {}", e),
                                }
                            } else {
                                println!("Invalid page id: {}", args[2]);
                            }
                        }

                        other => println!("Unknown page command: {}", other),
                    }
                }
                Err(e) => println!("✗ Could not open page file {}: {}", pages_file.display(), e),
            }
        } else {
            println!("Usage: debug [on|off|true|false|status]");
        }
    }

    fn handle_precompute_command(&self, args: &[&str]) {
        if args.is_empty() {
            println!("Usage: precompute save <name> <query>");
            return;
        }

        match args[0] {
            "save" => {
                if args.len() < 3 {
                    println!("Usage: precompute save <name> <query_string>");
                    return;
                }
                let name = args[1];
                let query = args[2..].join(" ");

                match self.db.save_precomputed_query(name, &query) {
                    Ok(_) => println!("✓ Query '{}' saved", name),
                    Err(e) => println!("✗ Error: {}", e),
                }
            }
            "list" => match self.db.list_precomputed_queries() {
                Ok(queries) => {
                    if queries.is_empty() {
                        println!("No precomputed queries.");
                    } else {
                        println!("\nPrecomputed queries:");
                        for q in queries {
                            println!("  • {}", q);
                        }
                        println!();
                    }
                }
                Err(e) => println!("✗ Error: {}", e),
            },
            _ => println!("Usage: precompute save|list"),
        }
    }

    fn handle_server_command(&mut self, args: &[&str]) {
        if args.is_empty() {
            println!("Usage: server start [addr] [--auth-token <token>]");
            return;
        }

        match args[0] {
            "start" => {
                // determine addr and optional token
                let mut addr = "0.0.0.0:7878".to_string();
                let mut auth_token: Option<String> = None;

                if args.len() > 1 && !args[1].starts_with("--") {
                    addr = args[1].to_string();
                }

                for i in 0..args.len() {
                    if args[i] == "--auth-token" && i + 1 < args.len() {
                        auth_token = Some(args[i + 1].to_string());
                    }
                }

                // launch server in background thread bound to this DB path
                let db_path = self.db.path().to_path_buf();
                let addr_clone = addr.clone();
                let token_clone = auth_token.clone();
                std::thread::spawn(move || {
                    if let Err(e) = crate::server::serve(db_path, &addr_clone, token_clone) {
                        eprintln!("Server error: {}", e);
                    }
                });

                println!("Server starting on http://{} (auth {})", addr, if auth_token.is_some() { "enabled" } else { "disabled" });
            }
            _ => println!("Unknown server command. Use: server start [addr] [--auth-token <token>]."),
        }
    }
}

fn parse_field_spec(spec: &str) -> FieldDef {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.is_empty() {
        return FieldDef::new("id", FieldType::String);
    }

    let name = parts[0];
    let field_type = match parts.get(1).copied().unwrap_or("string") {
        "string" | "String" => FieldType::String,
        "integer" | "Integer" | "int" => FieldType::Integer,
        "float" | "Float" => FieldType::Float,
        "boolean" | "Boolean" | "bool" => FieldType::Boolean,
        "reference" | "Reference" => FieldType::Reference,
        _ => FieldType::String,
    };

    let mut field = FieldDef::new(name, field_type);

    for part in &parts[2..] {
        match *part {
            "required" => field = field.required(),
            "nullable" => field = field.nullable(),
            ref target if field_type == FieldType::Reference => {
                field = field.references(*target);
            }
            _ => {}
        }
    }

    field
}

fn parse_value(raw: &str) -> Value {
    let cleaned = raw.trim();

    if cleaned.eq_ignore_ascii_case("null") {
        return Value::Null;
    }

    if cleaned.eq_ignore_ascii_case("true") {
        return Value::Boolean(true);
    }

    if cleaned.eq_ignore_ascii_case("false") {
        return Value::Boolean(false);
    }

    if let Ok(v) = cleaned.parse::<i64>() {
        return Value::Integer(v);
    }

    if let Ok(v) = cleaned.parse::<f64>() {
        return Value::Float(v);
    }

    if cleaned.contains('/') && cleaned.split('/').count() == 2 {
        return Value::Reference(cleaned.to_string());
    }

    Value::String(cleaned.to_string())
}

fn parse_query_condition(condition: &str) -> Option<Predicate> {
    if let Some((field, rest)) = condition.split_once('>') {
        if rest.starts_with('=') {
            let value_str = &rest[1..];
            Some(Predicate::GreaterOrEqual {
                field: field.to_string(),
                value: parse_value(value_str),
            })
        } else {
            let value_str = rest;
            Some(Predicate::GreaterThan {
                field: field.to_string(),
                value: parse_value(value_str),
            })
        }
    } else if let Some((field, rest)) = condition.split_once('<') {
        if rest.starts_with('=') {
            let value_str = &rest[1..];
            Some(Predicate::LessOrEqual {
                field: field.to_string(),
                value: parse_value(value_str),
            })
        } else {
            let value_str = rest;
            Some(Predicate::LessThan {
                field: field.to_string(),
                value: parse_value(value_str),
            })
        }
    } else if let Some((field, rest)) = condition.split_once("!=") {
        Some(Predicate::NotEquals {
            field: field.to_string(),
            value: parse_value(rest),
        })
    } else if let Some((field, value_str)) = condition.split_once('=') {
        Some(Predicate::Equals {
            field: field.to_string(),
            value: parse_value(value_str),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod shell_page_tests {
    use super::*;
    use crate::storage::page::{PageId, PAGE_SIZE};
    use crate::storage::page_manager::FilePageStorage;
    use crate::storage::page_manager::PageManager;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_db_path(prefix: &str) -> std::path::PathBuf {
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}.db", prefix, std::process::id(), start))
    }

    #[test]
    fn shell_page_persistence_single() {
        let path = unique_db_path("shell_page_single");
        fs::create_dir_all(&path).unwrap();

        // Open database (creates .kelp metadata dir)
        let db = Database::open_local(&path).unwrap();
        let mut shell = Shell::new(db.clone(), ShellConfig { debug: true, show_timing: false, track_storage: false });

        // Allocate page via shell command
        shell.handle_debug_command(&["page", "allocate", "Object"]);

        // Write a short string into page 1 via shell command
        shell.handle_debug_command(&["page", "write", "1", "hello-from-page"]);

        // Read back via PageManager to validate content was written
        let pages_file = path.join(".kelp").join("pages.db");
        let fps = FilePageStorage::new(&pages_file).unwrap();
        let pm = PageManager::new(Box::new(fps));
        let page = pm.read_page(PageId::new(1)).unwrap();
        assert!(page.data.starts_with(b"hello-from-page"));

        // Check file size multiple of PAGE_SIZE and expected size (superblock + 1 page)
        let meta = fs::metadata(&pages_file).unwrap();
        assert_eq!(meta.len() % PAGE_SIZE as u64, 0);
        assert_eq!(meta.len(), (2 * PAGE_SIZE) as u64);

        // Drop and reopen DB, then read again via newly opened PageManager
        drop(pm);
        drop(shell);
        drop(db);

        let db2 = Database::open_local(&path).unwrap();
        let mut shell2 = Shell::new(db2.clone(), ShellConfig { debug: true, show_timing: false, track_storage: false });

        // Verify reading through shell does not panic (we also programmatically verify)
        shell2.handle_debug_command(&["page", "read", "1"]);

        let fps2 = FilePageStorage::new(&pages_file).unwrap();
        let pm2 = PageManager::new(Box::new(fps2));
        let page2 = pm2.read_page(PageId::new(1)).unwrap();
        assert!(page2.data.starts_with(b"hello-from-page"));

        // Cleanup
        let _ = fs::remove_file(&pages_file);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn shell_page_persistence_multiple_pages() {
        let path = unique_db_path("shell_page_multi");
        fs::create_dir_all(&path).unwrap();

        let db = Database::open_local(&path).unwrap();
        let mut shell = Shell::new(db.clone(), ShellConfig { debug: true, show_timing: false, track_storage: false });

        shell.handle_debug_command(&["page", "allocate", "Object"]);
        shell.handle_debug_command(&["page", "allocate", "Object"]);

        shell.handle_debug_command(&["page", "write", "1", "AAAA"]);
        shell.handle_debug_command(&["page", "write", "2", "BBBB"]);

        let pages_file = path.join(".kelp").join("pages.db");
        let fps = FilePageStorage::new(&pages_file).unwrap();
        let pm = PageManager::new(Box::new(fps));

        let p1 = pm.read_page(PageId::new(1)).unwrap();
        let p2 = pm.read_page(PageId::new(2)).unwrap();
        assert!(p1.data.starts_with(b"AAAA"));
        assert!(p2.data.starts_with(b"BBBB"));

        // Expected file size: superblock + 2 pages
        let meta = fs::metadata(&pages_file).unwrap();
        assert_eq!(meta.len() % PAGE_SIZE as u64, 0);
        assert_eq!(meta.len(), (3 * PAGE_SIZE) as u64);

        // Reopen and re-verify
        drop(pm);
        drop(shell);
        drop(db);

        let db2 = Database::open_local(&path).unwrap();
        let fps2 = FilePageStorage::new(&pages_file).unwrap();
        let pm2 = PageManager::new(Box::new(fps2));
        let p1b = pm2.read_page(PageId::new(1)).unwrap();
        let p2b = pm2.read_page(PageId::new(2)).unwrap();
        assert!(p1b.data.starts_with(b"AAAA"));
        assert!(p2b.data.starts_with(b"BBBB"));

        // Cleanup
        let _ = fs::remove_file(&pages_file);
        let _ = fs::remove_dir_all(path);
    }
}
