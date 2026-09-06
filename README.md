# Kelp - Fast, Queryable, Object-Oriented Embedded Database

Kelp is a modern embedded database designed for developers who want the simplicity of an object-oriented data model with the power of a proper queryable database. It combines schema validation, type safety, and efficient storage in one elegant package.

> **Looking for a more developed tool? Check out [DAM](https://dam-pcp.web.app/).**
>
> [DAM](https://dam-pcp.web.app/) is a more developed data and project management tool from OneAM Labs. It provides a powerful CLI and storage ecosystem for working with projects, repositories, archives, and data.
>
> **GitHub:** [github.com/OneAM-Labs/dam](https://github.com/OneAM-Labs/dam)

## Features

✅ **Fast & Lightweight** - Embedded database, no external dependencies
✅ **Queryable** - Rich predicate-based queries (equality, comparisons, compound conditions)
✅ **Object-Oriented** - First-class support for strongly-typed objects with references
✅ **Schema Validation** - Define schemas with field constraints (required, nullable, types)
✅ **Interactive Shell** - Built-in REPL for exploration and development
✅ **Debugging Tools** - Performance metrics, storage tracking, execution timing
✅ **SDK Ready** - Simple, well-documented Rust API for embedding in your apps

## Installation

### Using DAM

The easiest way to get the Kelp source is with **[DAM](https://dam-pcp.web.app/)**.

If you already have DAM installed, import the Kelp repository directly:

```bash
dam import https://github.com/OneAM-Labs/kelp.git
```

You can also import a specific branch:

```bash
dam import --branch main https://github.com/OneAM-Labs/kelp.git
```

For more information about DAM, installation options, pre-built binaries, and manual installation from source, see the **[DAM README](https://github.com/OneAM-Labs/dam)**.

### Manual Build from Source

If you prefer to build Kelp manually from its source code, first import the repository with DAM:

```bash
dam import https://github.com/OneAM-Labs/kelp.git
```

Then enter the imported project and build it with Cargo:

```bash
cd kelp
cargo build --release
```

The resulting binary will be available at:

```text
target/release/kelp
```

> **Note:** For instructions on installing DAM itself, including downloading a pre-built binary with `curl`, Windows installation, or installing through the Arch Linux AUR, see the **[DAM README](https://github.com/OneAM-Labs/dam)**.

## Quick Start

Once Kelp is installed, you can create and manage a database using the CLI:

```bash
# Create a new database
kelp create ./my_app --name "My App"

# Launch interactive shell
kelp shell ./my_app

# View database info
kelp inspect ./my_app

# Delete when done
kelp delete ./my_app
```

## Quick Example

### Using the CLI Shell

```bash
$ kelp shell ./my_app

kelp> help
╔════════════════════════════════════════════════════════╗
║                  Available Commands                    ║
╚════════════════════════════════════════════════════════╝

SCHEMA MANAGEMENT:
  schema create <Name> [field:type[:required|nullable]]
    └─ Create a new object type
  schema list
    └─ List all schemas
  schema show <Name>
    └─ Display schema details
  ...

# Create a User schema
kelp> schema create User \
  id:string:required \
  name:string:required \
  email:string:required \
  age:integer

✓ Schema 'User' created

# Create a user object
kelp> object create User u1 \
  id=u1 \
  name=Alice \
  email=alice@example.com \
  age=30

✓ Created User/u1

# Query users by name
kelp> query User name=Alice
Query results (1 found):
  • User/u1

# List all users
kelp> object list User
Objects of type 'User' (1 total):
  • User/u1

# View database statistics
kelp> inspect
╔════════════════════════════════════════════════════════╗
║            Database Inspection Report                   ║
╚════════════════════════════════════════════════════════╝

Database Name:       My App
Path:                ./my_app
Storage Backend:     Local File

Statistics:
  Total Size:        2.45 KB
  Schemas:           1
  Object Types:      1
  Objects:           1

kelp> exit
Goodbye!
```

### Using the Rust SDK

```rust
use kelp_db::{Database, Schema, FieldDef, FieldType, Object, Value, Predicate};

fn main() -> kelp_db::Result<()> {
    // Create or open a database
    let db = Database::open_local("./my_app.db")?;

    // Define a User schema
    let user_schema = Schema::new("User")
        .add_field(FieldDef::new("id", FieldType::String).required())
        .add_field(FieldDef::new("name", FieldType::String).required())
        .add_field(FieldDef::new("email", FieldType::String).required())
        .add_field(FieldDef::new("age", FieldType::Integer));
    
    db.create_type(&user_schema)?;

    // Create a user
    let mut user = Object::new("User", "u1");
    user.set_field("id", Value::String("u1".to_string()));
    user.set_field("name", Value::String("Alice".to_string()));
    user.set_field("email", Value::String("alice@example.com".to_string()));
    user.set_field("age", Value::Integer(30));
    
    db.create_object(&user)?;

    // Retrieve by ID
    if let Some(retrieved) = db.get_object("User", "u1")? {
        println!("Found user: {:?}", retrieved.id);
    }

    // Query by field
    let by_age = db.query_by_field("User", "age", Value::Integer(30))?;
    println!("Users aged 30: {}", by_age.objects.len());

    // Advanced query with predicates
    let predicate = Predicate::gt("age", Value::Integer(25));
    let results = db.query("User", &predicate)?;
    println!("Users over 25: {}", results.objects.len());

    // Update a field
    db.update_field("User", "u1", "age", Value::Integer(31))?;

    // Delete an object
    db.delete_object("User", "u1")?;

    // Get database stats
    let summary = db.inspect()?;
    println!("Database: {} ({} bytes)", summary.name, summary.total_size_bytes);

    Ok(())
}
```

## CLI Commands Reference

### Database Management

```bash
# Create a new database
kelp create <path> [--name <database_name>]

# View database information
kelp inspect [<path>]     # Default: current directory

# Delete a database
kelp delete <path>

# Launch interactive shell
kelp shell [<path>] [--debug]
```

### Help System

Every command has detailed help with examples:

```bash
kelp help              # General help
kelp help create       # Help for create command
kelp help shell        # Help for shell command
```

## Shell Commands Reference

### Schema Commands

```
# Create a new object type
schema create <Name> [field:type[:required|nullable]]

Examples:
  schema create User name:string:required email:string:required age:integer
  schema create Product name:string:required price:float stock:integer

# List all schemes
schema list

# Show schema details
schema show User
```

### Object Commands

```
# Create an object
object create <Type> <Id> [field=value ...]

Examples:
  object create User u1 name=Alice email=alice@example.com age=30
  object create Product p1 name="Laptop" price=999.99 stock=50

# Get a specific object
object get <Type> <Id>

Examples:
  object get User u1
  object get Product p1

# List objects of a type
object list <Type> [--limit N]

Examples:
  object list User
  object list Product --limit 10

# Delete an object
object delete <Type> <Id>

Examples:
  object delete User u1
  object delete Product p1

# Update an object
object update <Type> <Id> field=value [...]

Examples:
  object update User u1 age=31 email=newemail@example.com
```

### Query Commands

```
# Query with field equality
query <Type> field=value [field=value ...]

Examples:
  query User name=Alice
  query Product price=999.99

# Query with comparisons
query <Type> field>value
query <Type> field<value
query <Type> field>=value
query <Type> field<=value
query <Type> field!=value

Examples:
  query User age>25
  query Product price<100
  query User email!=alice@example.com
```

### Database Commands

```
# Show database statistics
inspect

# Enable/disable debug mode (shows timing and storage changes)
debug [on|off|status]

# Save a named query for reuse
precompute save <name> <query>
precompute list

# Show help
help [command]

# Exit shell
exit
```

## SDK Documentation

### Core Types

#### `Database`

Main database interface for all operations.

```rust
// Open/create a local database
let db = Database::open_local("./data.db")?;

// Use in-memory database (testing)
let db = Database::memory()?;

// Set/get database name
db.set_name("My Database")?;
let name = db.name()?;

// Get statistics
let summary = db.inspect()?;
```

#### `Schema`

Defines the structure of object types.

```rust
let schema = Schema::new("User")
    .add_field(FieldDef::new("id", FieldType::String).required())
    .add_field(FieldDef::new("name", FieldType::String).required())
    .add_field(FieldDef::new("email", FieldType::String));

db.create_type(&schema)?;
```

#### `Object`

Represents an instance of a typed object.

```rust
let mut obj = Object::new("User", "u1");
obj.set_field("name", Value::String("Alice".to_string()));
obj.set_field("age", Value::Integer(30));

db.create_object(&obj)?;
```

#### `Value`

Atomic data types supported by Kelp.

```rust
Value::String("text".to_string())
Value::Integer(42)
Value::Float(3.14)
Value::Boolean(true)
Value::Null
Value::Reference("User/u1".to_string())  // Reference to another object
```

#### `Predicate`

Query conditions for filtering objects.

```rust
// Equality
Predicate::equals("name", Value::String("Alice".to_string()))

// Comparisons
Predicate::gt("age", Value::Integer(25))
Predicate::lt("price", Value::Float(100.0))
Predicate::gte("score", Value::Integer(80))
Predicate::lte("percentage", Value::Float(50.0))
Predicate::not_equals("status", Value::String("inactive".to_string()))

// Compound queries
let combined = Predicate::and(vec![
    Predicate::gt("age", Value::Integer(25)),
    Predicate::equals("active", Value::Boolean(true)),
]);
```

## Example: Building a Task Management App

### 1. Initialize the database

```bash
kelp create ./tasks_db --name "Task Manager"
kelp shell ./tasks_db
```

### 2. Define schemas

```
kelp> schema create User \
  id:string:required \
  username:string:required \
  email:string:required

kelp> schema create Task \
  id:string:required \
  title:string:required \
  description:string \
  assigned_to:reference:User \
  status:string:required \
  priority:integer

kelp> schema create TaskList \
  id:string:required \
  name:string:required \
  owner:reference:User
```

### 3. Create data

```
# Create users
kelp> object create User alice id=alice username=alice email=alice@example.com
kelp> object create User bob id=bob username=bob email=bob@example.com

# Create task lists
kelp> object create TaskList tl1 id=tl1 name="Work" owner=User/alice
kelp> object create TaskList tl2 id=tl2 name="Personal" owner=User/alice

# Create tasks
kelp> object create Task t1 \
  id=t1 \
  title="Implement auth" \
  description="Add user authentication" \
  assigned_to=User/alice \
  status=in_progress \
  priority=1

kelp> object create Task t2 \
  id=t2 \
  title="Design schema" \
  description="Design database schema" \
  assigned_to=User/bob \
  status=pending \
  priority=2
```

### 4. Query data

```
# Find all tasks assigned to Alice
kelp> query Task assigned_to=User/alice

# Find high-priority tasks
kelp> query Task priority>1

# Find all in-progress tasks
kelp> query Task status=in_progress

# List all users
kelp> object list User
```

## Performance & Debugging

### Enable Debug Mode

Debug mode shows timing and storage changes for each command:

```bash
# From CLI
kelp shell ./my_db --debug

# In shell
kelp> debug on
✓ Debug mode enabled (timing and storage tracking ON)

# Then run commands to see timing info
kelp> object create User u1 name=Alice
✓ Created User/u1
⏱  Execution time: 2.34ms
📦 Storage change: +512 bytes

kelp> query User name=Alice
Query results (1 found):
  • User/u1

⏱  Execution time: 0.45ms
```

### Inspect Database

Get detailed statistics about your database:

```bash
kelp> inspect
╔════════════════════════════════════════════════════════╗
║            Database Inspection Report                   ║
╚════════════════════════════════════════════════════════╝

Database Name:       My App
Path:                ./my_app
Storage Backend:     Local File

Statistics:
  Total Size:        45.67 KB
  Schemas:           3
  Object Types:      3
  Objects:           142

⚡ Precomputed Queries:
   • users_by_status
   • active_orders
```

### Precomputed Queries

Save frequently-used queries for reuse:

```bash
# Save a query
kelp> precompute save active_users name!=alice status=active

# List saved queries
kelp> precompute list

# Later queries will be optimized if you save them
```

## API Examples

### Bulk Operations

```rust
fn bulk_create_users(db: &Database) -> kelp_db::Result<()> {
    let users = vec![
        ("u1", "Alice", 30),
        ("u2", "Bob", 25),
        ("u3", "Charlie", 35),
    ];

    for (id, name, age) in users {
        let mut user = Object::new("User", id);
        user.set_field("name", Value::String(name.to_string()));
        user.set_field("age", Value::Integer(age));
        db.create_object(&user)?;
    }

    Ok(())
}
```

### Complex Queries

```rust
fn find_active_users_over_age(
    db: &Database,
    min_age: i64,
) -> kelp_db::Result<Vec<Object>> {
    let predicate = Predicate::and(vec![
        Predicate::gt("age", Value::Integer(min_age)),
        Predicate::equals("active", Value::Boolean(true)),
    ]);

    let results = db.query("User", &predicate)?;
    Ok(results.objects)
}
```

### Transaction Support

```rust
fn transfer_data(db: &Database) -> kelp_db::Result<()> {
    // Begin transaction
    db.begin_transaction()?;

    // Perform operations
    db.create_object(&user1)?;
    db.create_object(&user2)?;

    // Commit or rollback
    db.commit()?;
    // OR: db.rollback()?;

    Ok(())
}
```

## Architecture

### Storage Abstraction

Kelp uses a pluggable storage backend:

- **LocalStorage**: File-based storage (default)
- **MemoryStorage**: In-memory storage (testing)

Extend with your own backends by implementing `StorageBackend`.

### Query Engine

- Simple predicate-based queries with type checking
- Support for equality, comparisons, and compound (AND) predicates
- Efficient filtering and result collection

### Validation

- Schema validation at type definition time
- Object validation at creation/update time
- Automatic constraint checking (required, nullable, types)

## Roadmap

- [ ] Advanced query operators (OR, NOT, IN)
- [ ] Full-text search
- [ ] Indexing for faster queries
- [ ] Remote backend support
- [ ] Query optimization hints
- [ ] Aggregation functions (COUNT, SUM, AVG)
- [ ] Replication support
- [ ] GraphQL API
- [ ] Web-based dashboard

## Contributing

Contributions welcome! Please follow the existing code style and add tests for new features.

## License

Apache License - See LICENSE file for details

## Support

- 📖 Documentation: Full doc strings in source code
- 🐛 Issues: Report on GitHub

---

Built with ❤️ for developers who love simplicity and power.
