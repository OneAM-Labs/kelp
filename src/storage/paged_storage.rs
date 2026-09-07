use crate::object::{Object, ObjectId};
use crate::Result;
use crate::storage::binary_codec::BinaryEncoder;
use crate::storage::local::LocalStorage;
use crate::storage::location_index::ObjectLocationIndex;
use crate::storage::page::{Page, PageId, PageType, RecordLocation, SlotId, PAGE_SIZE};
use crate::storage::page_manager::{FilePageStorage, PageManager};
use crate::storage::slotted_page::SlottedPageLayout;
use crate::storage::wal::Wal;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent storage backend backed by fixed-size pages and a WAL.
pub struct PagedStorage {
    local: LocalStorage,
    pm: PageManager,
    wal: Wal,
    index: ObjectLocationIndex,
    base_dir: PathBuf,
    current_txn: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct WalPayload {
    op: String,
    type_name: String,
    object_id: String,
    data_b64: String,
}

impl PagedStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let base = path.as_ref().to_path_buf();
        let local = LocalStorage::new(&base)?;

        let pages_path = base.join(".kelp").join("pages.db");
        let fps = FilePageStorage::new(&pages_path)?;
        let mut pm = PageManager::new(Box::new(fps));

        let wal_path = base.join(".kelp").join("wal.log");
        let mut wal = Wal::new(wal_path)?;

        // Load persistent location index from .kelp/locations.json if present.
        let mut index = ObjectLocationIndex::new();
        let index_path = base.join(".kelp").join("locations.json");
        if index_path.exists() {
            if let Ok(bytes) = std::fs::read(&index_path) {
                if let Ok(entries) = serde_json::from_slice::<Vec<(String,String,u32,u16)>>(&bytes) {
                    for (t,id,p,s) in entries {
                        index.insert(&ObjectId::new(t.clone(), id.clone()), RecordLocation::new(PageId::new(p), SlotId::new(s)));
                    }
                }
            }
        }

        // Replay WAL for committed transactions to ensure pages/index
        if let Ok(records) = wal.read_all_records() {
            let mut committed = std::collections::HashSet::new();
            for r in &records {
                if r.record_type == crate::storage::wal::WalRecordType::Commit {
                    committed.insert(r.txn_id);
                }
            }

            for r in records.into_iter() {
                if !committed.contains(&r.txn_id) {
                    continue;
                }

                if let Ok(payload_str) = String::from_utf8(r.payload.clone()) {
                    if let Ok(p) = serde_json::from_str::<WalPayload>(&payload_str) {
                        let record_bytes = BASE64.decode(&p.data_b64).unwrap_or_default();
                        let obj_id = ObjectId::new(p.type_name.clone(), p.object_id.clone());

                        match r.record_type {
                            crate::storage::wal::WalRecordType::Insert => {
                                // If index already has this object, skip inserting duplicate
                                if !index.exists(&obj_id) {
                                    let slot = Self::insert_record_into_pages(&mut pm, &record_bytes)?;
                                    index.insert(&obj_id, slot);
                                }
                            }
                            crate::storage::wal::WalRecordType::Update => {
                                // Insert new copy and update index; try to delete old slot if present
                                let new_slot = Self::insert_record_into_pages(&mut pm, &record_bytes)?;
                                if let Some(old_loc) = index.get(&obj_id) {
                                    if let Ok(mut page) = pm.read_page(old_loc.page_id) {
                                        let mut layout = SlottedPageLayout::from_page(&page)?;
                                        let _ = layout.delete_record(&mut page, old_loc.slot_id);
                                        let _ = pm.write_page(&page);
                                    }
                                }
                                index.insert(&obj_id, new_slot);
                            }
                            crate::storage::wal::WalRecordType::Delete => {
                                if index.exists(&obj_id) {
                                    if let Some(loc) = index.get(&obj_id) {
                                        if let Ok(mut page) = pm.read_page(loc.page_id) {
                                            let mut layout = SlottedPageLayout::from_page(&page)?;
                                            let _ = layout.delete_record(&mut page, loc.slot_id);
                                            let _ = pm.write_page(&page);
                                        }
                                    }
                                    index.remove(&obj_id);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // persist index after replay
            Self::persist_index(&index, &base)?;
        }

        Ok(Self { local, pm, wal, index, base_dir: base, current_txn: None })
    }
    fn persist_index(index: &ObjectLocationIndex, base: &std::path::Path) -> Result<()> {
        // serialize to vector of tuples and write to .kelp/locations.json
        let mut entries: Vec<(String,String,u32,u16)> = Vec::new();
        for t in index.types() {
            for (id, loc) in index.get_all_by_type(&t) {
                entries.push((t.clone(), id, loc.page_id.as_u32(), loc.slot_id.as_u16()));
            }
        }

        let data = serde_json::to_vec(&entries)?;
        let idx_path = base.join(".kelp").join("locations.json");
        std::fs::create_dir_all(idx_path.parent().unwrap())?;
        std::fs::write(&idx_path, &data).map_err(|e| crate::Error::StorageError{reason: e.to_string()})?;
        Ok(())
    }

    fn insert_record_into_pages(pm: &mut PageManager, record: &[u8]) -> Result<RecordLocation> {
        // scan for an object page that can fit
        let last = (pm.size()? / PAGE_SIZE as u64) as u32;
        eprintln!("DEBUG insert_record_into_pages: pm.size bytes={}, last_page={}", pm.size()?, last);
        for pid in 1..=last {
            let page_id = PageId::new(pid);
            eprintln!("DEBUG checking page {}", pid);
            if let Ok(mut page) = pm.read_page(page_id) {
                eprintln!("DEBUG page {} type={:?}", pid, page.page_type);
                if page.page_type != PageType::Object {
                    continue;
                }
                let mut layout = SlottedPageLayout::from_page(&page)?;
                if layout.can_fit(record.len()) {
                    let slot = layout.insert_record(&mut page, record)?;
                    pm.write_page(&page)?;
                    return Ok(RecordLocation::new(page_id, slot));
                }
            }
        }

        // none found, allocate new object page
        let page_id = pm.allocate_page(PageType::Object)?;
        eprintln!("DEBUG allocate new object page => {}", page_id.as_u32());
        let mut page = pm.read_page(page_id)?;
        let mut layout = SlottedPageLayout::new();
        let slot = layout.insert_record(&mut page, record)?;
        pm.write_page(&page)?;
        Ok(RecordLocation::new(page_id, slot))
    }
}


impl crate::storage::StorageBackend for PagedStorage {
    fn put_schema(&mut self, schema: &crate::schema::Schema) -> Result<()> {
        self.local.put_schema(schema)
    }

    fn get_schema(&self, name: &str) -> Result<Option<crate::schema::Schema>> {
        self.local.get_schema(name)
    }

    fn list_schemas(&self) -> Result<Vec<String>> {
        self.local.list_schemas()
    }

    fn put_object(&mut self, object: &Object) -> Result<()> {
        // encode using schema
        let schema = self.local.get_schema(&object.id.type_name)?.ok_or(crate::Error::TypeNotFound{type_name: object.id.type_name.clone()})?;
        let encoded = BinaryEncoder::encode_object(object, &schema)?;
        // Determine transaction id: use current transaction if present
        let mut own_txn = false;
        let txn = match self.current_txn {
            Some(txn_id) => txn_id,
            None => {
                own_txn = true;
                self.wal.begin_transaction()?
            }
        };

        // Decide whether this is an insert or update
        if self.index.exists(&object.id) {
            // Update path
            let old_loc = self.index.get(&object.id).unwrap();
            // Try in-place overwrite when possible
            if let Ok(mut page) = self.pm.read_page(old_loc.page_id) {
                let mut layout = SlottedPageLayout::from_page(&page)?;
                if let Some(old_bytes) = layout.read_record(&page, old_loc.slot_id)? {
                    if encoded.len() <= old_bytes.len() {
                        // Log update
                        let payload = WalPayload { op: "update".to_string(), type_name: object.id.type_name.clone(), object_id: object.id.object_id.clone(), data_b64: BASE64.encode(&encoded) };
                        let payload_bytes = serde_json::to_vec(&payload)?;
                        self.wal.log_update(txn, payload_bytes)?;
                        self.wal.sync()?;

                        // Overwrite in-place
                        layout.overwrite_record(&mut page, old_loc.slot_id, &encoded)?;
                        self.pm.write_page(&page)?;
                        Self::persist_index(&self.index, &self.base_dir)?;

                        if own_txn {
                            self.wal.commit(txn)?;
                            self.wal.sync()?;
                            self.pm.sync()?;
                        }

                        return Ok(());
                    }
                }
            }

            // Otherwise allocate a new slot and remove the old one
            let payload = WalPayload { op: "update".to_string(), type_name: object.id.type_name.clone(), object_id: object.id.object_id.clone(), data_b64: BASE64.encode(&encoded) };
            let payload_bytes = serde_json::to_vec(&payload)?;
            self.wal.log_update(txn, payload_bytes)?;
            self.wal.sync()?;

            let new_loc = Self::insert_record_into_pages(&mut self.pm, &encoded)?;
            // delete old
            if let Ok(mut page) = self.pm.read_page(old_loc.page_id) {
                let mut layout = SlottedPageLayout::from_page(&page)?;
                layout.delete_record(&mut page, old_loc.slot_id)?;
                self.pm.write_page(&page)?;
            }

            self.index.insert(&object.id, new_loc);
            Self::persist_index(&self.index, &self.base_dir)?;

            if own_txn {
                self.wal.commit(txn)?;
                self.wal.sync()?;
                self.pm.sync()?;
            }

            return Ok(());
        }

        // Insert path
        let payload = WalPayload { op: "insert".to_string(), type_name: object.id.type_name.clone(), object_id: object.id.object_id.clone(), data_b64: BASE64.encode(&encoded) };
        let payload_bytes = serde_json::to_vec(&payload)?;
        self.wal.log_insert(txn, payload_bytes.clone())?;
        self.wal.sync()?;

        // apply to pages
        let loc = Self::insert_record_into_pages(&mut self.pm, &encoded)?;
        eprintln!("DEBUG put_object: inserted at page={} slot={}", loc.page_id.as_u32(), loc.slot_id.as_u16());
        self.index.insert(&object.id, loc);
        // persist index to disk
        Self::persist_index(&self.index, &self.base_dir)?;

        // commit if we created the txn
        if own_txn {
            self.wal.commit(txn)?;
            self.wal.sync()?;
            self.pm.sync()?;
        }

        Ok(())
    }

    fn get_object(&self, type_name: &str, object_id: &str) -> Result<Option<Object>> {
        let obj_id = ObjectId::new(type_name, object_id);
        if let Some(loc) = self.index.get(&obj_id) {
            let page = self.pm.read_page(loc.page_id)?;
            let layout = SlottedPageLayout::from_page(&page)?;
            if let Some(bytes) = layout.read_record(&page, loc.slot_id)? {
                eprintln!("DEBUG get_object: page={} slot={} bytes_len={} data={:?}", loc.page_id.as_u32(), loc.slot_id.as_u16(), bytes.len(), &bytes);
                let schema = self.local.get_schema(type_name)?.ok_or(crate::Error::TypeNotFound{type_name: type_name.to_string()})?;
                let obj = BinaryEncoder::decode_object_with_schema(&bytes, &schema)?;
                return Ok(Some(obj));
            }
        }
        Ok(None)
    }

    fn delete_object(&mut self, type_name: &str, object_id: &str) -> Result<()> {
        let obj_id = ObjectId::new(type_name, object_id);
        let loc = self.index.get(&obj_id).ok_or(crate::Error::ObjectNotFound{ type_name: type_name.to_string(), object_id: object_id.to_string() })?;
        // Use current transaction if present
        let mut own_txn = false;
        let txn = match self.current_txn {
            Some(txn_id) => txn_id,
            None => {
                own_txn = true;
                self.wal.begin_transaction()?
            }
        };

        let payload = WalPayload { op: "delete".to_string(), type_name: type_name.to_string(), object_id: object_id.to_string(), data_b64: "".to_string() };
        let payload_bytes = serde_json::to_vec(&payload)?;
        self.wal.log_delete(txn, payload_bytes)?;
        self.wal.sync()?;

        // apply delete
        let mut page = self.pm.read_page(loc.page_id)?;
        let mut layout = SlottedPageLayout::from_page(&page)?;
        layout.delete_record(&mut page, loc.slot_id)?;
        self.pm.write_page(&page)?;
        self.index.remove(&obj_id);
        Self::persist_index(&self.index, &self.base_dir)?;

        if own_txn {
            self.wal.commit(txn)?;
            self.wal.sync()?;
            self.pm.sync()?;
        }

        Ok(())
    }

    fn list_objects(&self, type_name: &str) -> Result<Vec<Object>> {
        let mut out = Vec::new();
        for (id, loc) in self.index.get_all_by_type(type_name) {
            if let Ok(Some(obj)) = self.get_object(type_name, &id) {
                out.push(obj);
            }
        }
        Ok(out)
    }

    fn object_exists(&self, type_name: &str, object_id: &str) -> Result<bool> {
        Ok(self.index.exists(&ObjectId::new(type_name, object_id)))
    }

    fn inspect(&self) -> Result<crate::storage::StorageSummary> {
        let mut s = self.local.inspect()?;
        s.storage_backend = "paged-storage".to_string();
        s.total_size_bytes = self.pm.size()?;
        s.schema_count = self.local.list_schemas()?.len();
        s.object_type_count = self.index.types().len();
        s.object_count = self.index.total_objects();
        Ok(s)
    }

    fn set_database_name(&mut self, name: &str) -> Result<()> {
        self.local.set_database_name(name)
    }

    fn database_name(&self) -> Result<String> {
        self.local.database_name()
    }

    fn save_precomputed_query(&mut self, name: &str, query: &str) -> Result<()> {
        self.local.save_precomputed_query(name, query)
    }

    fn list_precomputed_queries(&self) -> Result<Vec<String>> {
        self.local.list_precomputed_queries()
    }

    fn begin_transaction(&mut self) -> Result<()> {
        if self.current_txn.is_some() {
            return Err(crate::Error::TransactionError { reason: "Transaction already in progress".to_string() });
        }
        let txn = self.wal.begin_transaction()?;
        self.current_txn = Some(txn);
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if let Some(txn) = self.current_txn {
            self.wal.commit(txn)?;
            self.wal.sync()?;
            self.pm.sync()?;
            self.current_txn = None;
            Ok(())
        } else {
            Err(crate::Error::TransactionError { reason: "No transaction in progress".to_string() })
        }
    }

    fn rollback(&mut self) -> Result<()> {
        if let Some(txn) = self.current_txn {
            // We can mark the transaction rolled back in the WAL, but full undo is
            // not implemented in this prototype. Signal the rollback and clear state.
            let _ = self.wal.rollback(txn);
            self.current_txn = None;
            Err(crate::Error::TransactionError { reason: "Rollback is not supported for paged storage in this version".to_string() })
        } else {
            Err(crate::Error::TransactionError { reason: "No transaction in progress".to_string() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
        use crate::storage::StorageBackend;
    use std::fs;
    use crate::schema::{FieldDef, FieldType};

    fn unique_db() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let tmp = std::env::temp_dir().join(format!("kelp_paged_test_{}_{}", std::process::id(), nanos));
        let _ = fs::create_dir_all(&tmp);
        tmp
    }

    #[test]
    fn smoke_create_and_get() {
        let path = unique_db();
        let mut s = PagedStorage::new(&path).unwrap();

        let schema = crate::schema::Schema::new("User").add_field(FieldDef::new("id", FieldType::String));
        s.put_schema(&schema).unwrap();

        let mut obj = Object::new("User", "u1");
        obj.set_field("id", crate::Value::String("u1".to_string()));
        s.put_object(&obj).unwrap();

        let got = s.get_object("User", "u1").unwrap();
        assert!(got.is_some());
    }

    #[test]
    fn restart_replays_wal_insert_update_delete() {
        let path = unique_db();

        // create storage and insert object
        {
            let mut s = PagedStorage::new(&path).unwrap();
            let schema = crate::schema::Schema::new("Note")
                .add_field(FieldDef::new("id", FieldType::String))
                .add_field(FieldDef::new("text", FieldType::String));
            s.put_schema(&schema).unwrap();

            let mut n = Object::new("Note", "n1");
            n.set_field("id", crate::Value::String("n1".to_string()));
            n.set_field("text", crate::Value::String("first".to_string()));
            s.put_object(&n).unwrap();

            // update
            n.set_field("text", crate::Value::String("updated".to_string()));
            s.put_object(&n).unwrap();

            // delete
            s.delete_object("Note", "n1").unwrap();
        }

        // reopen and replay WAL - object should be absent
        {
            let s2 = PagedStorage::new(&path).unwrap();
            let got = s2.get_object("Note", "n1").unwrap();
            assert!(got.is_none());
        }
    }

    #[test]
    fn restart_persists_insert() {
        let path = unique_db();
        // insert and close
        {
            let mut s = PagedStorage::new(&path).unwrap();
            let schema = crate::schema::Schema::new("Task")
                .add_field(FieldDef::new("id", FieldType::String))
                .add_field(FieldDef::new("title", FieldType::String));
            s.put_schema(&schema).unwrap();

            let mut t = Object::new("Task", "t1");
            t.set_field("id", crate::Value::String("t1".to_string()));
            t.set_field("title", crate::Value::String("task".to_string()));
            s.put_object(&t).unwrap();
        }

        // reopen
        {
            let s2 = PagedStorage::new(&path).unwrap();
            let got = s2.get_object("Task", "t1").unwrap();
            assert!(got.is_some());
            let obj = got.unwrap();
            assert_eq!(obj.get_field("title").unwrap(), &crate::Value::String("task".to_string()));
        }
    }
}
