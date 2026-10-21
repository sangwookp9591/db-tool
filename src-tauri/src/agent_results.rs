//! Private, bounded tool results. No rows or handle IDs are persisted to disk.
use dbcore::LockExt;
use dbcore::DbError;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const DEFAULT_BYTES: usize = 256 * 1024;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_HANDLES: usize = 32;
const IDLE: Duration = Duration::from_secs(600);

struct Counter {
    bytes: usize,
    limit: usize,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            return Err(io::Error::other("Result budget exceeded"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn size(value: &impl Serialize, limit: usize) -> Option<usize> {
    let mut writer = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut writer, value).ok()?;
    Some(writer.bytes)
}

struct Entry {
    id: String,
    client: String,
    credential: String,
    session: String,
    profile: String,
    value: Value,
    encoded: Option<Vec<u8>>,
    bytes: usize,
    touched: Instant,
    expiry: Option<tokio::task::AbortHandle>,
}
impl Drop for Entry {
    fn drop(&mut self) {
        if let Some(task) = self.expiry.take() {
            task.abort();
        }
    }
}
#[derive(Default, Clone)]
pub struct Store(Arc<Mutex<VecDeque<Entry>>>);
impl Store {
    pub fn clear_session(&self, session: &str) {
        self.0.lock_or_recover().retain(|e| e.session != session);
    }
    pub fn clear_client(&self, client: &str) {
        self.0.lock_or_recover().retain(|e| e.client != client);
    }
    pub fn clear(&self) {
        self.0.lock_or_recover().clear();
    }
    pub fn insert(
        &self,
        client: &str,
        credential: &str,
        session: &str,
        profile: &str,
        value: Value,
    ) -> Option<String> {
        let encoded = size(&value, MAX_BYTES)?;
        fn heap(value: &Value) -> usize {
            std::mem::size_of::<Value>()
                + match value {
                    Value::String(s) => s.capacity(),
                    Value::Array(a) => {
                        a.capacity() * std::mem::size_of::<Value>()
                            + a.iter().map(heap).sum::<usize>()
                    }
                    Value::Object(o) => o
                        .iter()
                        .map(|(k, v)| k.capacity() + 128 + heap(v))
                        .sum::<usize>(),
                    _ => 0,
                }
        }
        let bytes = encoded.saturating_add(heap(&value));
        if bytes > MAX_BYTES {
            return None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut entries = self.0.lock_or_recover();
            entries.retain(|e| e.touched.elapsed() < IDLE);
            let mut used: usize = entries.iter().map(|e| e.bytes).sum();
            while entries.len() >= MAX_HANDLES || used + bytes > MAX_BYTES {
                used -= entries.pop_front()?.bytes;
            }
            entries.push_back(Entry {
                id: id.clone(),
                client: client.into(),
                credential: credential.into(),
                session: session.into(),
                profile: profile.into(),
                value,
                encoded: None,
                bytes,
                touched: Instant::now(),
                expiry: None,
            });
        }
        let weak = Arc::downgrade(&self.0);
        let expiry_id = id.clone();
        let expiry = tokio::spawn(async move {
            let mut delay = IDLE;
            loop {
                tokio::time::sleep(delay).await;
                let Some(store) = weak.upgrade() else {
                    break;
                };
                let mut entries = store.lock_or_recover();
                let Some(entry) = entries.iter().find(|e| e.id == expiry_id) else {
                    break;
                };
                if entry.touched.elapsed() >= IDLE {
                    entries.retain(|e| e.id != expiry_id);
                    break;
                }
                delay = IDLE.saturating_sub(entry.touched.elapsed());
            }
        })
        .abort_handle();
        if let Some(entry) = self.0.lock_or_recover().iter_mut().find(|e| e.id == id) {
            entry.expiry = Some(expiry);
        } else {
            expiry.abort();
        }
        Some(id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn page(
        &self,
        id: &str,
        client: &str,
        credential: &str,
        session: &str,
        profile: &str,
        offset: usize,
        rows: usize,
        bytes: usize,
        byte_offset: Option<usize>,
    ) -> Result<Value, DbError> {
        let mut entries = self.0.lock_or_recover();
        entries.retain(|e| e.touched.elapsed() < IDLE);
        let index = entries
            .iter()
            .position(|e| {
                e.id == id
                    && e.client == client
                    && e.credential == credential
                    && e.session == session
                    && e.profile == profile
            })
            .ok_or_else(|| DbError::query("Result handle expired or unavailable"))?;
        let Some(mut entry) = entries.remove(index) else {
            return Err(DbError::internal("Cached result unavailable"));
        };
        entry.touched = Instant::now();
        let page = if let Some(offset) = byte_offset {
            if entry.encoded.is_none() {
                let mut data = Vec::new();
                serde_json::to_writer(&mut data, &entry.value)
                    .map_err(|_| DbError::internal("Cannot encode cached result"))?;
                entry.encoded = Some(data);
            }
            match entry.encoded.as_deref() {
                Some(data) => byte_page(data, offset, bytes),
                None => Err(DbError::internal("Cached result unavailable")),
            }
        } else {
            Ok(project(&entry.value, offset, rows, bytes))
        };
        entries.push_back(entry);
        page
    }
}

// JSON byte pages let a client recover a cell larger than a normal row page without replaying SQL.
fn byte_page(data: &[u8], offset: usize, bytes: usize) -> Result<Value, DbError> {
    let raw = std::str::from_utf8(data).map_err(|_| DbError::internal("Invalid cached JSON"))?;
    if offset > data.len() || !raw.is_char_boundary(offset) {
        return Err(DbError::query("Invalid JSON byte offset"));
    }
    let mut end = offset
        .saturating_add(bytes.saturating_sub(4096) / 6)
        .min(data.len());
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    Ok(
        json!({"encoding":"json_utf8_fragment","byte_offset":offset,"data":&raw[offset..end],"next_byte_offset":if end < data.len() {Some(end)} else {None},"total_bytes":data.len(),"truncated":end < data.len()}),
    )
}

pub fn project(value: &Value, offset: usize, rows: usize, bytes: usize) -> Value {
    if value.get("error").is_some() && size(value, bytes.saturating_sub(1024)).is_some() {
        return value.clone();
    }
    struct Budget {
        left: usize,
        rows: usize,
        offset: usize,
        seen: usize,
        truncated: bool,
        omitted: Vec<usize>,
    }
    fn walk(value: &Value, key: &str, b: &mut Budget) -> Value {
        if let Value::Object(object) = value {
            let mut out = Map::new();
            for (key, value) in object {
                if b.left < key.len() + 8 {
                    b.truncated = true;
                    break;
                }
                b.left -= key.len() + 8;
                out.insert(key.clone(), walk(value, key, b));
            }
            if let Some(nodes) = out.get("nodes").and_then(Value::as_array) {
                let ids: Vec<String> = nodes
                    .iter()
                    .filter_map(|node| node["id"].as_str().map(str::to_owned))
                    .collect();
                if let Some(rels) = out.get_mut("rels").and_then(Value::as_array_mut) {
                    let before = rels.len();
                    rels.retain(|rel| {
                        ["start", "end"].iter().all(|key| {
                            rel[*key]
                                .as_str()
                                .is_some_and(|id| ids.iter().any(|kept| kept == id))
                        })
                    });
                    b.truncated |= before != rels.len();
                }
            }
            if b.truncated
                && ["table", "documents", "graph"]
                    .iter()
                    .any(|k| object.contains_key(*k))
            {
                out.insert("truncated".into(), json!(true));
            }
            Value::Object(out)
        } else if let Value::Array(values) = value {
            let items = matches!(key, "rows" | "documents" | "nodes" | "rels");
            let mut out = Vec::new();
            for value in values {
                if items {
                    b.seen += 1;
                    if b.seen <= b.offset {
                        continue;
                    }
                    if b.rows == 0 {
                        b.truncated = true;
                        continue;
                    }
                    if let Some(size) = size(value, b.left.saturating_sub(2)) {
                        b.left -= size + 2;
                        b.rows -= 1;
                        out.push(value.clone());
                    } else {
                        // Never silently substitute part of a typed cell; fetch its original JSON with byte_offset.
                        b.truncated = true;
                        b.rows -= 1;
                        if b.omitted.len() < 1000 {
                            b.omitted.push(b.seen - 1);
                        }
                    }
                } else {
                    if b.left < 4 {
                        b.truncated = true;
                        break;
                    }
                    b.left -= 2;
                    out.push(walk(value, "", b));
                }
            }
            Value::Array(out)
        } else if let Some(size) = size(value, b.left) {
            b.left -= size;
            value.clone()
        } else {
            b.truncated = true;
            Value::Null
        }
    }
    let mut budget = Budget {
        left: bytes.saturating_sub(4096),
        rows,
        offset,
        seen: 0,
        truncated: false,
        omitted: Vec::new(),
    };
    let mut result = walk(value, "", &mut budget);
    if let Some(object) = result.as_object_mut() {
        object.insert("truncated".into(), json!(budget.truncated));
        if !budget.omitted.is_empty() {
            object.insert("omitted_items".into(), json!(budget.omitted));
            object.insert(
                "oversize_recovery".into(),
                json!("get_result_page with byte_offset=0; continue using next_byte_offset"),
            );
        }
        object.insert(
            "next_offset".into(),
            json!(if offset.saturating_add(rows) < budget.seen {
                Some(offset.saturating_add(rows))
            } else {
                None
            }),
        );
        object.insert("total_items".into(), json!(budget.seen));
    }
    if budget.truncated && !result.is_object() {
        result = json!({"items":result,"truncated":true});
    }
    if size(&result, bytes.saturating_sub(1024)).is_none() {
        json!({"truncated":true,"reason":"metadata_byte_limit"})
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn byte_budget_evicts_and_client_cleanup_drops_entries() {
        let store = Store::default();
        let value = json!({"payload":"x".repeat(1024*1024)});
        let first = store.insert("a", "t", "s", "p", value.clone()).unwrap();
        for _ in 0..18 {
            store.insert("a", "t", "s", "p", value.clone()).unwrap();
        }
        assert!(
            store
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.bytes)
                .sum::<usize>()
                <= MAX_BYTES
        );
        assert!(store
            .page(&first, "a", "t", "s", "p", 0, 200, DEFAULT_BYTES, None)
            .is_err());
        store.clear_client("a");
        assert!(store.0.lock_or_recover().is_empty());
    }
    #[tokio::test]
    async fn caps_owner_lru_ttl_cleanup_and_huge_utf8() {
        let store = Store::default();
        let raw = json!({"results":[{"table":{"columns":[],"rows":(0..450).map(|i|json!([i])).collect::<Vec<_>>()}}]});
        let id = store.insert("a", "token", "s", "p", raw.clone()).unwrap();
        let page = store
            .page(&id, "a", "token", "s", "p", 200, 200, DEFAULT_BYTES, None)
            .unwrap();
        assert_eq!(page["results"][0]["table"]["rows"][0], json!([200]));
        assert_eq!(
            page["results"][0]["table"]["rows"]
                .as_array()
                .unwrap()
                .len(),
            200
        );
        for (a, t, s, p) in [
            ("b", "token", "s", "p"),
            ("a", "new", "s", "p"),
            ("a", "token", "x", "p"),
            ("a", "token", "s", "x"),
        ] {
            assert!(store
                .page(&id, a, t, s, p, 0, 200, DEFAULT_BYTES, None)
                .is_err());
        }
        let huge = json!({"results":[{"table":{"rows":[["한\"\n".repeat(100000)]]}}]});
        let preview = project(&huge, 0, 200, DEFAULT_BYTES);
        assert_eq!(preview["truncated"], true);
        assert!(size(&preview, DEFAULT_BYTES).is_some());
        let array = project(&json!(["x".repeat(DEFAULT_BYTES)]), 0, 200, DEFAULT_BYTES);
        assert_eq!(array["truncated"], true);
        assert!(size(&array, DEFAULT_BYTES).is_some());
        let graph = project(
            &json!({"results":[{"graph":{"nodes":[{"id":"a","props":{"large":"x".repeat(9000)}},{"id":"b"}],"rels":[{"start":"a","end":"b"}]}}]}),
            0,
            200,
            8192,
        );
        assert_eq!(
            graph["results"][0]["graph"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(graph["results"][0]["graph"]["rels"]
            .as_array()
            .unwrap()
            .is_empty());
        let mut collected = String::new();
        let mut offset = 0;
        loop {
            let page =
                byte_page(&serde_json::to_vec(&huge).unwrap(), offset, DEFAULT_BYTES).unwrap();
            assert!(size(&page, DEFAULT_BYTES).is_some());
            collected.push_str(page["data"].as_str().unwrap());
            if let Some(next) = page["next_byte_offset"].as_u64() {
                offset = next as usize;
            } else {
                break;
            }
        }
        assert_eq!(serde_json::from_str::<Value>(&collected).unwrap(), huge);
        for _ in 0..MAX_HANDLES {
            store.insert("a", "token", "s", "p", raw.clone()).unwrap();
        }
        assert!(store
            .page(&id, "a", "token", "s", "p", 0, 200, DEFAULT_BYTES, None)
            .is_err());
        store
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|e| e.touched = Instant::now() - IDLE);
        assert!(store.0.lock_or_recover().len() <= MAX_HANDLES);
        let expired = store.0.lock_or_recover()[0].id.clone();
        assert!(store
            .page(
                &expired,
                "a",
                "token",
                "s",
                "p",
                0,
                200,
                DEFAULT_BYTES,
                None
            )
            .is_err());
        store.insert("a", "token", "s", "p", raw).unwrap();
        store.clear_session("s");
        assert!(store.0.lock_or_recover().is_empty());
    }
}
