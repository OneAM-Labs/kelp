use crate::{Database, Result};
use crate::object::Object;
use crate::value::Value as KelpValue;
use crate::query::{Predicate, QueryResult};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::thread;
use tiny_http::Header;

/// Simple HTTP server exposing a minimal JSON API for the database.
pub fn serve(path: PathBuf, addr: &str, auth_token: Option<String>) -> Result<()> {
    let db = Database::open(path.clone())?;
    let server = tiny_http::Server::http(addr).map_err(|e| crate::Error::Internal(format!("Failed to bind server: {}", e)))?;

    eprintln!("Kelp HTTP API listening on http://{} (auth {})", addr, if auth_token.is_some() { "enabled" } else { "disabled" });

    for request in server.incoming_requests() {
        let db = db.clone();
        let token = auth_token.clone();
        thread::spawn(move || {
            if let Err(e) = handle_request(db, request, token) {
                eprintln!("HTTP handler error: {}", e);
            }
        });
    }

    Ok(())
}

fn json_to_kelp_value(v: &JsonValue) -> Option<KelpValue> {
    match v {
        JsonValue::String(s) => Some(KelpValue::String(s.clone())),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(KelpValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Some(KelpValue::Float(f))
            } else {
                None
            }
        }
        JsonValue::Bool(b) => Some(KelpValue::Boolean(*b)),
        JsonValue::Null => Some(KelpValue::Null),
        _ => None,
    }
}

fn json_response(body: &serde_json::Value, status: u16) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let s = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    tiny_http::Response::from_string(s)
        .with_status_code(status)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
}

fn handle_request(db: Database, mut req: tiny_http::Request, auth_token: Option<String>) -> Result<()> {
    let url = req.url().to_string();
    let method = req.method().as_str();

    // Log incoming requests for debugging
    eprintln!("HTTP request: {} {}", method, url);

    // Basic token check: expect header `Authorization: Bearer <token>` when token configured
    if let Some(ref token) = auth_token {
        let ok = req.headers().iter().any(|h| {
            let field_name = format!("{}", h.field).to_lowercase();
            if field_name == "authorization" {
                // header value is ascii-backed; format to String
                let s = format!("{}", h.value);
                return s.trim() == format!("Bearer {}", token);
            }
            false
        });

        if !ok {
            let _ = req.respond(tiny_http::Response::from_string("Unauthorized").with_status_code(401));
            return Ok(());
        }
    }

    // healthcheck
    if method == "GET" && url == "/health" {
        let body = serde_json::json!({"status":"ok"});
        let _ = req.respond(json_response(&body, 200));
        return Ok(());
    }

    if method == "POST" && url == "/put" {
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        if let Ok(json) = serde_json::from_str::<JsonValue>(&body) {
            if let (Some(t), Some(id), Some(fields)) = (
                json.get("type").and_then(|v| v.as_str()),
                json.get("id").and_then(|v| v.as_str()),
                json.get("fields"),
            ) {
                let mut obj = Object::new(t, id);
                if let JsonValue::Object(map) = fields {
                    for (k, v) in map {
                        if let Some(kv) = json_to_kelp_value(v) {
                            obj.set_field(k.clone(), kv);
                        }
                    }
                }

                // Try create, fallback to update
                let res = db.create_object(&obj).or_else(|_| db.update_object(&obj));
                if res.is_ok() {
                    let body = serde_json::json!({"status":"ok","object":obj});
                    let _ = req.respond(json_response(&body, 200));
                } else {
                    let body = serde_json::json!({"status":"error","reason":format!("Failed to put object")});
                    let _ = req.respond(json_response(&body, 500));
                }
                return Ok(());
            }
        }
        let _ = req.respond(tiny_http::Response::from_string("Invalid body").with_status_code(400));
        return Ok(());
    }

    if method == "GET" && url.starts_with("/get") {
        // expect query like /get?type=User&id=u1
        if let Some(q) = url.split('?').nth(1) {
            let mut type_name = None;
            let mut object_id = None;
            for part in q.split('&') {
                let mut kv = part.splitn(2, '=');
                if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                    if k == "type" { type_name = Some(v); }
                    if k == "id" { object_id = Some(v); }
                }
            }

            if let (Some(t), Some(id)) = (type_name, object_id) {
                if let Ok(Some(obj)) = db.get_object(t, id) {
                    let body = serde_json::json!({"status":"ok","object":obj});
                    let _ = req.respond(json_response(&body, 200));
                    return Ok(());
                } else {
                    let body = serde_json::json!({"status":"not_found"});
                    let _ = req.respond(json_response(&body, 404));
                    return Ok(());
                }
            }
        }
        let _ = req.respond(tiny_http::Response::from_string("Bad request").with_status_code(400));
        return Ok(());
    }

    if method == "GET" && url.starts_with("/list") {
        if let Some(q) = url.split('?').nth(1) {
            let mut type_name = None;
            for part in q.split('&') {
                let mut kv = part.splitn(2, '=');
                if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                    if k == "type" { type_name = Some(v); }
                }
            }
            if let Some(t) = type_name {
                if let Ok(list) = db.list_objects(t) {
                    let body = serde_json::json!({"status":"ok","objects": list.objects});
                    let _ = req.respond(json_response(&body, 200));
                    return Ok(());
                }
            }
        }
        let _ = req.respond(json_response(&serde_json::json!({"status":"error","reason":"bad request"}), 400));
        return Ok(());
    }

    // Create a schema: POST /schema with JSON {name:..., fields:[{name,type,required,nullable},...]}
    if method == "POST" && url == "/schema" {
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        if let Ok(json) = serde_json::from_str::<JsonValue>(&body) {
            if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
                let mut schema = crate::schema::Schema::new(name);
                if let Some(JsonValue::Array(fields)) = json.get("fields") {
                    for f in fields {
                        if let JsonValue::Object(map) = f {
                            let fname = map.get("name").and_then(|v| v.as_str()).unwrap_or("id");
                            let ftype = map.get("type").and_then(|v| v.as_str()).unwrap_or("string");
                            let required = map.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                            let nullable = map.get("nullable").and_then(|v| v.as_bool()).unwrap_or(false);
                            let ft = match ftype.to_lowercase().as_str() {
                                "string" => crate::schema::FieldType::String,
                                "integer" => crate::schema::FieldType::Integer,
                                "float" => crate::schema::FieldType::Float,
                                "boolean" => crate::schema::FieldType::Boolean,
                                "reference" => crate::schema::FieldType::Reference,
                                _ => crate::schema::FieldType::String,
                            };
                            let mut fd = crate::schema::FieldDef::new(fname, ft);
                            if required { fd = fd.required(); }
                            if nullable { fd = fd.nullable(); }
                            schema = schema.add_field(fd);
                        }
                    }
                }

                if let Err(e) = db.create_type(&schema) {
                    let body = serde_json::json!({"status":"error","reason":format!("{}", e)});
                    let _ = req.respond(json_response(&body, 500));
                } else {
                    let body = serde_json::json!({"status":"ok","schema":schema});
                    let _ = req.respond(json_response(&body, 200));
                }
                return Ok(());
            }
        }
        let _ = req.respond(json_response(&serde_json::json!({"status":"error","reason":"invalid body"}), 400));
        return Ok(());
    }

    // List schemas
    if method == "GET" && url == "/schemas" {
        match db.list_types() {
            Ok(types) => {
                let body = serde_json::json!({"status":"ok","schemas": types});
                let _ = req.respond(json_response(&body, 200));
            }
            Err(e) => {
                let body = serde_json::json!({"status":"error","reason": format!("{}", e)});
                let _ = req.respond(json_response(&body, 500));
            }
        }
        return Ok(());
    }

    // Query endpoint: POST /query {type:..., filters: {field: value, ...}}
    if method == "POST" && url == "/query" {
        let mut body_str = String::new();
        let _ = req.as_reader().read_to_string(&mut body_str);
        if let Ok(json) = serde_json::from_str::<JsonValue>(&body_str) {
            if let Some(t) = json.get("type").and_then(|v| v.as_str()) {
                let mut predicates = Vec::new();
                if let Some(JsonValue::Object(map)) = json.get("filters") {
                    for (k, v) in map {
                        if let Some(kv) = json_to_kelp_value(v) {
                            predicates.push(Predicate::equals(k.clone(), kv));
                        }
                    }
                }

                let combined = if predicates.is_empty() {
                    None
                } else if predicates.len() == 1 {
                    Some(predicates.into_iter().next().unwrap())
                } else {
                    Some(Predicate::and(predicates))
                };

                let res = if let Some(pred) = combined {
                    db.query(t, &pred)
                } else {
                    db.list_objects(t).map(|qr| qr)
                        .map(|qr| QueryResult::new(qr.objects))
                };

                match res {
                    Ok(qr) => {
                        let body = serde_json::json!({"status":"ok","objects": qr.objects});
                        let _ = req.respond(json_response(&body, 200));
                    }
                    Err(e) => {
                        let body = serde_json::json!({"status":"error","reason": format!("{}", e)});
                        let _ = req.respond(json_response(&body, 500));
                    }
                }
                return Ok(());
            }
        }
        let _ = req.respond(json_response(&serde_json::json!({"status":"error","reason":"invalid body"}), 400));
        return Ok(());
    }

    // Transaction endpoints
    if method == "POST" && url == "/txn/begin" {
        let res = db.begin_transaction();
        let status = if res.is_ok() { 200 } else { 500 };
        let _ = req.respond(tiny_http::Response::from_string("{}").with_status_code(status));
        return Ok(());
    }

    if method == "POST" && url == "/txn/commit" {
        let res = db.commit();
        let status = if res.is_ok() { 200 } else { 500 };
        let _ = req.respond(tiny_http::Response::from_string("{}").with_status_code(status));
        return Ok(());
    }

    if method == "POST" && url == "/txn/rollback" {
        let res = db.rollback();
        let status = if res.is_ok() { 200 } else { 500 };
        let _ = req.respond(tiny_http::Response::from_string("{}").with_status_code(status));
        return Ok(());
    }

    let _ = req.respond(tiny_http::Response::from_string("Not Found").with_status_code(404));
    Ok(())
}
