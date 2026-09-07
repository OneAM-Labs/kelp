use kelp_db::{Database, FieldDef, FieldType, Object, Schema, Value};
use rand::{distributions::Alphanumeric, Rng, SeedableRng};
use rand::rngs::StdRng;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

fn random_string(rng: &mut StdRng, len: usize) -> String {
    rng.sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // Usage: cargo run --example load_test -- [count] [threads] [backend]
    // backend: mem | paged (default: mem)

    let total: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10000000000);
    let threads: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or_else(|| num_cpus::get());
    let backend = args.get(3).map(|s| s.as_str()).unwrap_or("mem");
    let db_path_arg = args.get(4).map(|s| s.as_str());

    println!("Load test: total={} threads={} backend={}", total, threads, backend);

    // Open DB
    let db = if backend == "paged" {
        let db_dir: PathBuf = if let Some(p) = db_path_arg {
            PathBuf::from(p)
        } else {
            std::env::temp_dir().join(format!("kelp_load_test_{}", std::process::id()))
        };

        std::fs::create_dir_all(&db_dir).expect("create db dir");
        println!("Using paged DB at: {}", db_dir.display());
        println!("Inspect .kelp folder at: {}/.kelp", db_dir.display());
        Database::open(&db_dir).expect("open paged db")
    } else {
        Database::memory().expect("open memory db")
    };

    // Create schema
    let schema = Schema::new("Item")
        .add_field(FieldDef::new("id", FieldType::String).required())
        .add_field(FieldDef::new("name", FieldType::String))
        .add_field(FieldDef::new("value", FieldType::Integer));

    db.create_type(&schema).expect("create schema");

    let start = Instant::now();

    let db = Arc::new(db);
    let per_thread = (total + threads - 1) / threads;
    let successes = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for t in 0..threads {
        let db = Arc::clone(&db);
        let succ = Arc::clone(&successes);
        let fail = Arc::clone(&failures);
        let start_idx = t * per_thread;
        let end_idx = ((t + 1) * per_thread).min(total);
        if start_idx >= end_idx { break; }

        let handle = thread::spawn(move || {
            let mut rng = StdRng::from_entropy();
            for i in start_idx..end_idx {
                let id = format!("t{}_{}", t, i);
                let name = random_string(&mut rng, 12);
                let value: i64 = rng.gen_range(0..1_000_000);

                let mut obj = Object::new("Item", &id);
                obj.set_field("id", Value::String(id.clone()));
                obj.set_field("name", Value::String(name));
                obj.set_field("value", Value::Integer(value));

                match db.create_object(&obj) {
                    Ok(_) => { succ.fetch_add(1, Ordering::Relaxed); }
                    Err(_) => { fail.fetch_add(1, Ordering::Relaxed); }
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread join");
    }

    let elapsed = start.elapsed();
    println!("Done. elapsed: {:.3}s", elapsed.as_secs_f64());

    let succ = successes.load(Ordering::Relaxed);
    let fail = failures.load(Ordering::Relaxed);
    println!("Successes: {}  Failures: {}", succ, fail);

    // Verify count (best-effort)
    let count = db.count_objects("Item").unwrap_or(0);
    println!("Stored objects reported by DB: {}", count);
}

