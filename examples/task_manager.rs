//! Example: Task Management Application
//!
//! This example demonstrates how to use Kelp as the backend for a simple task management app.
//!
//! Run with: cargo run --example task_manager --release
//!
//! Features demonstrated:
//! - Creating schemas
//! - CRUD operations
//! - Querying with predicates
//! - Filtering and sorting results
//! - Database inspection

use kelp_db::{Database, FieldDef, FieldType, Object, Predicate, Schema, Value};

#[derive(Debug)]
struct Task {
    id: String,
    title: String,
    description: String,
    priority: i64,  // 1 = high, 2 = medium, 3 = low
    status: String, // "pending", "in_progress", "completed"
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔══════════════════════════════════════════╗");
    println!("║    Kelp Task Manager Example             ║");
    println!("╚══════════════════════════════════════════╝\n");

    // Create or open the database
    let db_path = "./example_task_db";
    let db = Database::open(db_path)?;

    println!(
        "📦 Database: {} (path: {})",
        db.name().unwrap_or_default(),
        db_path
    );

    // Initialize schema if not already done
    initialize_schema(&db)?;

    // Create some example tasks
    create_example_tasks(&db)?;

    // Query and display tasks
    display_all_tasks(&db)?;
    display_high_priority_tasks(&db)?;
    display_pending_tasks(&db)?;

    // Update a task
    update_task_status(&db, "task1", "in_progress")?;
    println!("\n✓ Updated task1 to 'in_progress'");

    // Show database stats
    show_database_stats(&db)?;

    println!("\n✓ Example completed successfully!");
    println!(
        "\nTip: Run 'kelp shell {}' to explore the database interactively",
        db_path
    );

    Ok(())
}

/// Initialize the Task schema
fn initialize_schema(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    // Check if Task schema already exists
    if db.get_type("Task")?.is_some() {
        println!("✓ Task schema already exists");
        return Ok(());
    }

    let task_schema = Schema::new("Task")
        .add_field(FieldDef::new("id", FieldType::String).required())
        .add_field(FieldDef::new("title", FieldType::String).required())
        .add_field(FieldDef::new("description", FieldType::String))
        .add_field(FieldDef::new("priority", FieldType::Integer).required())
        .add_field(FieldDef::new("status", FieldType::String).required());

    db.create_type(&task_schema)?;
    println!("✓ Created Task schema");

    Ok(())
}

/// Create some example tasks
fn create_example_tasks(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let tasks = vec![
        Task {
            id: "task1".to_string(),
            title: "Implement user authentication".to_string(),
            description: "Add login/signup functionality".to_string(),
            priority: 1,
            status: "pending".to_string(),
        },
        Task {
            id: "task2".to_string(),
            title: "Design database schema".to_string(),
            description: "Plan table structure and relationships".to_string(),
            priority: 1,
            status: "in_progress".to_string(),
        },
        Task {
            id: "task3".to_string(),
            title: "Write documentation".to_string(),
            description: "Create user guide and API docs".to_string(),
            priority: 2,
            status: "pending".to_string(),
        },
        Task {
            id: "task4".to_string(),
            title: "Setup CI/CD pipeline".to_string(),
            description: "Configure GitHub Actions".to_string(),
            priority: 2,
            status: "pending".to_string(),
        },
        Task {
            id: "task5".to_string(),
            title: "Fix mobile layout".to_string(),
            description: "Ensure responsive design on mobile".to_string(),
            priority: 3,
            status: "completed".to_string(),
        },
    ];

    println!("\n📝 Creating example tasks...");
    for task in tasks {
        let mut obj = Object::new("Task", &task.id);
        obj.set_field("id", Value::String(task.id.clone()));
        obj.set_field("title", Value::String(task.title));
        obj.set_field("description", Value::String(task.description));
        obj.set_field("priority", Value::Integer(task.priority));
        obj.set_field("status", Value::String(task.status));

        match db.create_object(&obj) {
            Ok(_) => println!("  ✓ {}", task.id),
            Err(_) => println!("  → {} (already exists)", task.id),
        }
    }

    Ok(())
}

/// Display all tasks
fn display_all_tasks(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 All Tasks:");
    let results = db.list_objects("Task")?;
    for obj in &results.objects {
        let title = obj
            .get_field("title")
            .and_then(|v| v.as_string())
            .unwrap_or("(no title)");
        let status = obj
            .get_field("status")
            .and_then(|v| v.as_string())
            .unwrap_or("(unknown)");
        println!("  • {} [{}] - {}", obj.id.object_id, status, title);
    }

    Ok(())
}

/// Display high-priority tasks
fn display_high_priority_tasks(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔥 High Priority Tasks (priority = 1):");
    let predicate = Predicate::equals("priority", Value::Integer(1));
    let results = db.query("Task", &predicate)?;

    if results.objects.is_empty() {
        println!("  (no high priority tasks)");
    } else {
        for obj in &results.objects {
            let title = obj
                .get_field("title")
                .and_then(|v| v.as_string())
                .unwrap_or("(no title)");
            println!("  • {} - {}", obj.id.object_id, title);
        }
    }

    Ok(())
}

/// Display pending tasks
fn display_pending_tasks(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⏱️  Pending Tasks:");
    let predicate = Predicate::equals("status", Value::String("pending".to_string()));
    let results = db.query("Task", &predicate)?;

    if results.objects.is_empty() {
        println!("  (no pending tasks)");
    } else {
        for obj in &results.objects {
            let title = obj
                .get_field("title")
                .and_then(|v| v.as_string())
                .unwrap_or("(no title)");
            let priority = obj
                .get_field("priority")
                .and_then(|v| {
                    if let Value::Integer(p) = v {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            let priority_label = match priority {
                1 => "HIGH",
                2 => "MED",
                _ => "LOW",
            };
            println!("  • {} [{}] - {}", obj.id.object_id, priority_label, title);
        }
    }

    Ok(())
}

/// Update a task's status
fn update_task_status(
    db: &Database,
    task_id: &str,
    new_status: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    db.update_field(
        "Task",
        task_id,
        "status",
        Value::String(new_status.to_string()),
    )?;

    Ok(())
}

/// Show database statistics
fn show_database_stats(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let summary = db.inspect()?;

    println!("\n📊 Database Statistics:");
    println!("  Name:          {}", summary.name);
    println!("  Size:          {} bytes", summary.total_size_bytes);
    println!("  Schemas:       {}", summary.schema_count);
    println!("  Object Types:  {}", summary.object_type_count);
    println!("  Objects:       {}", summary.object_count);

    Ok(())
}
