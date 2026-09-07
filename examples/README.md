# Kelp Example: Task Manager

This example demonstrates using Kelp as an embedded persistent database backed by fixed-size pages.

Run the example:

```bash
cargo run --example task_manager --release
```

Behavior:
- Creates/opens `./example_task_db` using Kelp's page-backed storage (.kelp/pages.db).
- Defines a `Task` schema (if missing).
- Inserts example tasks via the public `Database` API.
- Performs queries and updates using the public API.

Verify persistence:
1. Run the example once to create tasks.
2. Re-run the example; it should detect existing tasks and not duplicate them (tasks persisted in `.kelp/pages.db`).

Library usage:

```rust
use kelp_db::Database;
let db = Database::open("./my_app_db")?;
// create schema, objects, query, update, delete
```

HTTP API (curl):

- Start the server for the example DB directory:

```bash
kelp serve ./example_task_db 0.0.0.0:7878
```

- Create/insert an object via curl:

```bash
curl -X POST http://127.0.0.1:7878/put -H "Content-Type: application/json" \
	-d '{"type":"Task","id":"t100","fields":{"title":"Example task","status":"pending"}}'
```

- Retrieve an object:

```bash
curl "http://127.0.0.1:7878/get?type=Task&id=t100"
```

- List objects of a type:

```bash
curl "http://127.0.0.1:7878/list?type=Task"
```

