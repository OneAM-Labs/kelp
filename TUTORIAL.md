# Kelp Quick Start Tutorial

## 5-Minute Quick Start

### 1. Create Your First Database

```bash
kelp create ./demo --name "My Demo"
```

You'll see:
```
✓ Creating database at: ./demo
✓ Database name set to: My Demo
✓ Database initialized successfully!

Next steps:
  kelp shell ./demo # Launch the interactive shell
  kelp inspect ./demo # View database details
```

### 2. Launch the Interactive Shell

```bash
kelp shell ./demo
```

This opens an interactive prompt:
```
╔════════════════════════════════════════════════════════╗
║          Kelp Database Shell v0.2                      ║
╚════════════════════════════════════════════════════════╝

Database: My Demo
Path:     ./demo
Size:     13 bytes

Type 'help' for commands. Type 'exit' to quit.

kelp>
```

### 3. Define a Schema

Type this command in the shell:

```
kelp> schema create User name:string:required email:string:required age:integer
```

Response:
```
✓ Schema 'User' created
```

### 4. Create Objects

```
kelp> object create User u1 name=Alice email=alice@example.com age=30
```

Response:
```
✓ Created User/u1
```

Create a few more:
```
kelp> object create User u2 name=Bob email=bob@example.com age=25
kelp> object create User u3 name=Charlie email=charlie@example.com age=35
```

### 5. Query Your Data

Find Alice:
```
kelp> query User name=Alice
Query results (1 found):
  • User/u1
```

Find users over age 30:
```
kelp> query User age>30
Query results (1 found):
  • User/u3
```

List all users:
```
kelp> object list User
Objects of type 'User' (3 total):
  • User/u1
  • User/u2
  • User/u3
```

### 6. Inspect Database

See your database statistics:
```
kelp> inspect
╔════════════════════════════════════════════════════════╗
║            Database Inspection Report                   ║
╚════════════════════════════════════════════════════════╝

Database Name:       My Demo
Path:                ./demo
Storage Backend:     local-filesystem

Statistics:
  Total Size:        1.23 KB
  Schemas:           1
  Object Types:      1
  Objects:           3
```

### 7. Exit

```
kelp> exit
Goodbye!
```

## Common Workflows

### Workflow 1: Task Management App

Create a task management system:

```bash
kelp create ./tasks --name "Task Manager"
kelp shell ./tasks
```

Then in the shell:

```
# Define schemas
kelp> schema create Project \
  id:string:required \
  name:string:required \
  description:string

kelp> schema create Task \
  id:string:required \
  title:string:required \
  project:reference:Project \
  status:string:required \
  priority:integer

# Create projects
kelp> object create Project p1 id=p1 name="My Project" description="Build an app"
kelp> object create Project p2 id=p2 name="Learning" description="Learn Rust"

# Create tasks
kelp> object create Task t1 \
  id=t1 \
  title="Setup project" \
  project=Project/p1 \
  status=todo \
  priority=1

kelp> object create Task t2 \
  id=t2 \
  title="Read chapter 1" \
  project=Project/p2 \
  status=in_progress \
  priority=2

# Query tasks for a project
kelp> query Task project=Project/p1
Query results (1 found):
  • Task/t1

# Find high-priority tasks
kelp> query Task priority>1
Query results (1 found):
  • Task/t2

# Update task status
kelp> object update Task t1 status=in_progress
```

### Workflow 2: Simple Contact Book

```bash
kelp create ./contacts --name "Contact Book"
kelp shell ./contacts
```

```
# Define Contact schema
kelp> schema create Contact \
  id:string:required \
  name:string:required \
  email:string \
  phone:string \
  favorite:boolean

# Add contacts
kelp> object create Contact c1 id=c1 name=Alice email=alice@example.com phone="555-1234" favorite=true
kelp> object create Contact c2 id=c2 name=Bob email=bob@example.com phone="555-5678" favorite=false
kelp> object create Contact c3 id=c3 name=Charlie phone="555-9012" favorite=true

# List all contacts
kelp> object list Contact

# Find your favorite contacts
kelp> query Contact favorite=true

# Search by name (exact match)
kelp> query Contact name=Alice

# Get full contact info
kelp> object get Contact c1
```

### Workflow 3: Developing with the Rust SDK

Create `Cargo.toml`:

```toml
[package]
name = "kelp-app"
version = "0.1.0"
edition = "2021"

[dependencies]
kelp_db = "0.2"
```

Create `src/main.rs`:

```rust
use kelp_db::{Database, Schema, FieldDef, FieldType, Object, Value, Predicate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open database
    let db = Database::open_local("./app.db")?;

    // Create schema
    let schema = Schema::new("Note")
        .add_field(FieldDef::new("id", FieldType::String).required())
        .add_field(FieldDef::new("title", FieldType::String).required())
        .add_field(FieldDef::new("content", FieldType::String))
        .add_field(FieldDef::new("priority", FieldType::Integer));
    
    db.create_type(&schema)?;

    // Create notes
    let mut note1 = Object::new("Note", "n1");
    note1.set_field("id", Value::String("n1".to_string()));
    note1.set_field("title", Value::String("Learn Rust".to_string()));
    note1.set_field("priority", Value::Integer(1));
    db.create_object(&note1)?;

    // Query high-priority notes
    let results = db.query("Note", &Predicate::equals("priority", Value::Integer(1)))?;
    for note in &results.objects {
        println!("High-priority note: {}", note.id);
    }

    // Get stats
    let stats = db.inspect()?;
    println!("Database has {} objects", stats.object_count);

    Ok(())
}
```

Run with: `cargo run`

## Performance & Debugging

### Enable Debug Mode to See Timing

```bash
kelp shell ./demo --debug
```

Then commands will show timing and storage changes:

```
kelp> object create User u4 name=Dave email=dave@example.com
✓ Created User/u4
⏱  Execution time: 1.23ms
📦 Storage change: +256 bytes

kelp> query User name=Dave
Query results (1 found):
  • User/u4

⏱  Execution time: 0.34ms
```

### Check Database Size

```bash
kelp inspect ./demo
```

Look at the "Total Size" field to see your database size.

### Save Frequent Queries

For queries you'll use often:

```
kelp> precompute save high_priority "priority>1"
kelp> precompute list
```

## Tips & Tricks

1. **Field Types Supported**: string, integer, float, boolean, reference
2. **Reference Fields**: Create relationships between objects
3. **Comparison Operators**: `>`, `<`, `>=`, `<=`, `!=` in queries
4. **Compound Queries**: Separate conditions with spaces to AND them together
5. **Help**: Type `help` anytime to see available commands

## Next Steps

- Read the full [README.md](./README.md) for advanced features
- Check out the [example app](./examples/task_manager.rs)
- Explore the [SDK documentation](./src/database.rs)
- Try building your own app!

---

**Built for developers who value simplicity and power.** 🚀
