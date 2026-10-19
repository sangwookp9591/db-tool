use dbcore::*;
use driver_redis::RedisDriver;
use redis::{aio::MultiplexedConnection, Value as Resp};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

fn request(text: &str) -> ExecRequest {
    serde_json::from_value(json!({"text":text,"read_only":true})).unwrap()
}
fn cfg() -> ConnectConfig {
    ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "56379".into()),
            ("db".into(), "0".into()),
        ]),
        secret: Some("testredis".into()),
        connect_timeout_ms: 2000,
    }
}
fn key_id(key: &str) -> String {
    json!(["key", key.as_bytes()]).to_string()
}
fn page(offset: u64, limit: u32) -> PageRequest {
    PageRequest {
        offset,
        limit,
        filter: vec![],
    }
}
fn rows(result: &ResultSet) -> &Vec<Vec<Value>> {
    &result.table.as_ref().unwrap().rows
}
async fn run(session: &dyn Session, text: &str) -> ResultSet {
    session
        .execute(&request(text), CancelToken::new())
        .await
        .unwrap()
        .remove(0)
}
async fn raw(con: &mut MultiplexedConnection, name: &str, args: &[&[u8]]) -> Resp {
    let mut cmd = redis::cmd(name);
    for a in args {
        cmd.arg(*a);
    }
    cmd.query_async(con).await.unwrap()
}
async fn blocked(con: &mut MultiplexedConnection) -> usize {
    let s: String = redis::from_redis_value(raw(con, "INFO", &[b"clients"]).await).unwrap();
    s.lines()
        .find_map(|l| l.strip_prefix("blocked_clients:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}
#[tokio::test]
async fn real_server_contract() {
    let Ok(uri) = std::env::var("AXDB_TEST_REDIS") else {
        eprintln!("SKIP real Redis contract: AXDB_TEST_REDIS is unset");
        return;
    };
    // Never let this test write to a user server, including the user's port 6379.
    assert_eq!(
        uri, "redis://:testredis@127.0.0.1:56379/0",
        "Only the isolated axdb-test-redis endpoint is permitted"
    );
    let mut con = redis::Client::open(uri)
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let driver = RedisDriver::new();
    let session: Arc<dyn Session> = Arc::from(driver.connect(&cfg()).await.unwrap());
    assert!(session
        .server_info()
        .await
        .unwrap()
        .starts_with("redis_version:7."));
    session.ping().await.unwrap();
    let mut bad = cfg();
    bad.fields.insert("port".into(), "1".into());
    assert_eq!(
        driver.connect(&bad).await.err().unwrap().kind,
        ErrorKind::Connection
    );
    bad = cfg();
    bad.secret = Some("wrong-private-password".into());
    let e = driver.connect(&bad).await.err().unwrap();
    assert_eq!(e.kind, ErrorKind::Auth);
    assert!(!format!("{e:?}").contains("wrong-private-password"));
    let mut named = cfg();
    named.fields.insert("username".into(), "default".into());
    driver.connect(&named).await.unwrap().ping().await.unwrap();
    let prefix = format!(
        "axdbt:r1:{}:",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let keys: Vec<String> = [
        "string",
        "hash",
        "list",
        "set",
        "zset",
        "stream",
        "binary",
        "large",
        "empty",
        "wait",
        "write",
        "nested:child",
        "colon:",
    ]
    .iter()
    .map(|s| format!("{prefix}{s}"))
    .collect();
    raw(&mut con, "SET", &[keys[0].as_bytes(), b"hello world"]).await;
    raw(&mut con, "EXPIRE", &[keys[0].as_bytes(), b"120"]).await;
    raw(
        &mut con,
        "HSET",
        &[
            keys[1].as_bytes(),
            b"field1",
            b"value1",
            b"field2",
            b"value2",
            b"field3",
            b"value3",
        ],
    )
    .await;
    raw(
        &mut con,
        "RPUSH",
        &[keys[2].as_bytes(), b"first", b"second", b"third"],
    )
    .await;
    raw(&mut con, "SADD", &[keys[3].as_bytes(), b"a", b"b", b"c"]).await;
    raw(
        &mut con,
        "ZADD",
        &[keys[4].as_bytes(), b"1.25", b"a", b"2.5", b"b", b"3", b"c"],
    )
    .await;
    for i in [b"a", b"b", b"c"] {
        raw(&mut con, "XADD", &[keys[5].as_bytes(), b"*", b"field", i]).await;
    }
    raw(&mut con, "SET", &[keys[6].as_bytes(), &[0, 255, 128, 10]]).await;
    let large = "큰 값 ".repeat(100_000);
    raw(&mut con, "SET", &[keys[7].as_bytes(), large.as_bytes()]).await;
    raw(&mut con, "SET", &[keys[8].as_bytes(), b""]).await;
    for i in [11, 12] {
        raw(&mut con, "SET", &[keys[i].as_bytes(), b"nested"]).await;
    }
    for (i, kind) in ["string", "hash", "list", "set", "zset", "stream"]
        .iter()
        .enumerate()
    {
        let result = session.browse(&key_id(&keys[i]), page(0, 2)).await.unwrap();
        assert!(
            result
                .message
                .as_ref()
                .unwrap()
                .contains(&format!("TYPE: {kind}; TTL:")),
            "{result:?}"
        );
        assert_eq!(result.row_count, if i == 0 { 1 } else { 2 });
        assert_eq!(result.truncated, i != 0);
        let next = session.browse(&key_id(&keys[i]), page(2, 2)).await.unwrap();
        assert_eq!(next.row_count, if i == 0 { 0 } else { 1 });
        assert!(!next.truncated);
    }
    let z = session
        .browse(&key_id(&keys[4]), page(0, 10))
        .await
        .unwrap();
    assert_eq!(z.table.as_ref().unwrap().columns[1].name, "score");
    assert!(rows(&z).contains(&vec![Value::Text("a".into()), Value::Text("1.25".into())]));
    let bin = session.browse(&key_id(&keys[6]), page(0, 1)).await.unwrap();
    assert_eq!(
        rows(&bin)[0][0],
        Value::Bytes {
            len: 4,
            hex: "00ff800a".into()
        }
    );
    let big = session.browse(&key_id(&keys[7]), page(0, 1)).await.unwrap();
    assert_eq!(rows(&big)[0][0], Value::Text(large));
    let empty = session.browse(&key_id(&keys[8]), page(0, 1)).await.unwrap();
    assert_eq!(rows(&empty)[0][0], Value::Text("".into()));
    let missing = session.browse(&key_id(&keys[9]), page(0, 1)).await.unwrap();
    assert_eq!(missing.row_count, 0);
    assert!(missing.message.unwrap().contains("TTL: -2"));
    let scan_id = json!(["scan", prefix.as_bytes(), format!("{prefix}*"), 0]).to_string();
    let scan = session.browse(&scan_id, page(0, 2)).await.unwrap();
    assert_eq!(scan.row_count, 2);
    assert!(scan.truncated);
    let all = session.browse(&scan_id, page(0, 100)).await.unwrap();
    assert_eq!(all.row_count, 11);
    assert!(!all.truncated);
    let children = session.list_children(Some(&scan_id)).await.unwrap();
    let folder = children.iter().find(|n| n.label == "nested:").unwrap();
    assert!(folder.has_children);
    let leaf = session.list_children(Some(&folder.id)).await.unwrap();
    assert_eq!(leaf[0].label, "child");
    assert!(leaf[0]
        .actions
        .iter()
        .any(|a| matches!(a.kind, NodeActionKind::Browse)));
    assert_eq!(
        run(session.as_ref(), &format!("HGETALL {}", keys[1]))
            .await
            .table
            .unwrap()
            .columns[0]
            .name,
        "field"
    );
    assert_eq!(
        run(
            session.as_ref(),
            &format!("ZRANGE {} 0 -1 WITHSCORES", keys[4])
        )
        .await
        .table
        .unwrap()
        .columns[1]
            .name,
        "score"
    );
    assert_eq!(
        run(session.as_ref(), &format!("XRANGE {} - +", keys[5]))
            .await
            .row_count,
        3
    );
    for command in ["SCAN", "HSCAN", "SSCAN", "ZSCAN"] {
        let text = if command == "SCAN" {
            format!("SCAN 0 MATCH {prefix}* COUNT 100")
        } else {
            let i = match command {
                "HSCAN" => 1,
                "SSCAN" => 3,
                _ => 4,
            };
            format!("{command} {} 0 COUNT 100", keys[i])
        };
        assert!(run(session.as_ref(), &text)
            .await
            .message
            .unwrap()
            .contains("SCAN cursor:"));
    }
    let mut req = request(&format!("LRANGE {} 0 -1", keys[2]));
    req.row_limit = 1;
    let limited = session.execute(&req, CancelToken::new()).await.unwrap();
    assert!(limited[0].truncated);
    assert_eq!(limited[0].row_count, 1);
    req = request("SET $key $value");
    req.read_only = false;
    req.params = json!({"key":keys[10],"value":"hello \"quoted\"\r\nFLUSHALL"})
        .as_object()
        .unwrap()
        .clone();
    session.execute(&req, CancelToken::new()).await.unwrap();
    req = request("GET $key");
    req.params = json!({"key":keys[10]}).as_object().unwrap().clone();
    let value = session.execute(&req, CancelToken::new()).await.unwrap();
    assert_eq!(
        rows(&value[0])[0][0],
        Value::Text("hello \"quoted\"\r\nFLUSHALL".into())
    );
    for text in [
        format!("SET {} denied", keys[10]),
        "FLUSHALL".into(),
        "FLUSHDB".into(),
        "CONFIG GET *".into(),
        "SCRIPT FLUSH".into(),
        "EVAL 'return 1' 0".into(),
        "EVAL_RO 'return 1' 0".into(),
        format!("GETDEL {}", keys[10]),
        "KEYS *".into(),
        "SORT k STORE target".into(),
    ] {
        assert_eq!(
            session
                .execute(&request(&text), CancelToken::new())
                .await
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
    }
    assert_eq!(
        rows(&run(session.as_ref(), &format!("GET {}", keys[10])).await)[0][0],
        Value::Text("hello \"quoted\"\r\nFLUSHALL".into())
    );
    assert_eq!(
        rows(&run(session.as_ref(), &format!("EXISTS {}", keys[0])).await)[0][0],
        Value::Int("1".into())
    );
    let e = session
        .execute(&request("GET testredis extra"), CancelToken::new())
        .await
        .unwrap_err();
    assert!(!format!("{e:?}").contains("testredis"));
    assert_eq!(
        session
            .preview(&request("SET a b"), CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        session.schema_graph(None).await.unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        session.expand("x", 1).await.unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let mut filtered = page(0, 1);
    filtered.filter.push(("x".into(), Value::Null));
    assert_eq!(
        session.browse(&scan_id, filtered).await.unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let many_prefix = format!("{prefix}many:");
    let many: Vec<String> = (0..225).map(|i| format!("{many_prefix}{i:03}")).collect();
    for k in &many {
        raw(&mut con, "SET", &[k.as_bytes(), b"page"]).await;
    }
    let many_id = json!(["scan", many_prefix.as_bytes(), format!("{many_prefix}*"), 0]).to_string();
    let first = session.list_children(Some(&many_id)).await.unwrap();
    assert_eq!(first.len(), 201);
    let continuation = first.iter().find(|n| n.label == "More keys…").unwrap();
    let rest = session.list_children(Some(&continuation.id)).await.unwrap();
    assert_eq!(rest.len(), 25);
    let mut ids: BTreeSet<String> = first
        .iter()
        .filter(|n| n.kind == "key")
        .map(|n| n.id.clone())
        .collect();
    ids.extend(rest.into_iter().map(|n| n.id));
    assert_eq!(ids.len(), 225);
    let last = session.browse(&many_id, page(224, 2)).await.unwrap();
    assert_eq!(last.row_count, 1);
    assert!(!last.truncated);
    assert_eq!(
        session
            .browse(&many_id, page(100_001, 1))
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Query
    );
    let mut binary_key = prefix.as_bytes().to_vec();
    binary_key.extend([255, 0]);
    raw(&mut con, "SET", &[&binary_key, b"binary key"]).await;
    let binary_id = json!(["key", binary_key]).to_string();
    assert_eq!(
        rows(&session.browse(&binary_id, page(0, 1)).await.unwrap())[0][0],
        Value::Text("binary key".into())
    );
    raw(&mut con, "DEL", &[&binary_key]).await;
    raw(
        &mut con,
        "DEL",
        &many.iter().map(|k| k.as_bytes()).collect::<Vec<_>>(),
    )
    .await;
    let literal_prefix = format!("{prefix}glob[*?]:");
    let literal_key = format!("{literal_prefix}child");
    raw(&mut con, "SET", &[literal_key.as_bytes(), b"literal glob"]).await;
    let literal_id = json!(["scan", literal_prefix.as_bytes(), "*", 0]).to_string();
    assert_eq!(
        session.list_children(Some(&literal_id)).await.unwrap()[0].label,
        "child"
    );
    raw(&mut con, "DEL", &[literal_key.as_bytes()]).await;
    let baseline = blocked(&mut con).await;
    req = request(&format!("BLPOP {} 0", keys[9]));
    req.read_only = false;
    req.timeout_ms = 100;
    assert_eq!(
        session
            .execute(&req, CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    for _ in 0..100 {
        if blocked(&mut con).await == baseline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        blocked(&mut con).await,
        baseline,
        "timeout must close blocked connection"
    );
    session.ping().await.unwrap();
    req.timeout_ms = 10_000;
    let stop = CancelToken::new();
    let task_stop = stop.clone();
    let task_session = session.clone();
    let task = tokio::spawn(async move { task_session.execute(&req, task_stop).await });
    for _ in 0..100 {
        if blocked(&mut con).await > baseline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        blocked(&mut con).await > baseline,
        "cancellation test must reach the server"
    );
    stop.cancel();
    assert_eq!(task.await.unwrap().unwrap_err().kind, ErrorKind::Cancelled);
    for _ in 0..100 {
        if blocked(&mut con).await == baseline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        blocked(&mut con).await,
        baseline,
        "cancel must close blocked connection"
    );
    session.ping().await.unwrap();
    let stop = CancelToken::new();
    stop.cancel();
    let mut cancelled_write = request(&format!("SET {} cancelled", keys[9]));
    cancelled_write.read_only = false;
    assert_eq!(
        session
            .execute(&cancelled_write, stop)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    assert_eq!(
        raw(&mut con, "EXISTS", &[keys[9].as_bytes()]).await,
        Resp::Int(0)
    );
    session.close().await;
    assert_eq!(
        session.ping().await.unwrap_err().kind,
        ErrorKind::Connection
    );
    raw(
        &mut con,
        "DEL",
        &keys.iter().map(|k| k.as_bytes()).collect::<Vec<_>>(),
    )
    .await;
}
