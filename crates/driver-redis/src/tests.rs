use super::*;
use serde_json::json;
use std::collections::BTreeMap;

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
#[test]
fn tokenizer_parameters_and_capabilities() {
    let params =
        json!({"key":"axdbt:key with spaces","value":"x\r\nFLUSHALL","count":2,"bad":null})
            .as_object()
            .unwrap()
            .clone();
    assert_eq!(
        tokenize(r#"SET $key "hello 'world'""#, &params).unwrap(),
        vec![
            b"SET".to_vec(),
            b"axdbt:key with spaces".to_vec(),
            b"hello 'world'".to_vec()
        ]
    );
    assert_eq!(tokenize(r#"GET "$key""#, &params).unwrap()[1], b"$key");
    assert_eq!(
        tokenize(r#"SET key "\x00\xff\n""#, &params).unwrap()[2],
        vec![0, 255, 10]
    );
    assert_eq!(
        tokenize("SET k $value", &params).unwrap()[2],
        b"x\r\nFLUSHALL"
    );
    assert_eq!(tokenize("SET k ''", &params).unwrap()[2], Vec::<u8>::new());
    for s in [
        "",
        "GET 'bad",
        "GET a\nFLUSHALL",
        "GET \\",
        "GET \\x0",
        "GET $missing",
        "GET $bad",
        "$key a",
    ] {
        assert!(tokenize(s, &params).is_err(), "{s:?}");
    }
    for c in [
        "SET",
        "DEL",
        "GETDEL",
        "GETEX",
        "FLUSHDB",
        "FLUSHALL",
        "CONFIG",
        "SCRIPT",
        "EVAL",
        "EVAL_RO",
        "FCALL_RO",
        "XREADGROUP",
        "SORT",
        "CLIENT",
        "MIGRATE",
        "RESTORE",
        "AUTH",
        "SELECT",
    ] {
        assert!(!read_command(c), "{c}");
    }
    let m = RedisDriver::new().manifest().clone();
    assert_eq!(m.paradigm, Paradigm::KeyValue);
    assert_eq!(m.capabilities.preview_changes, PreviewSupport::None);
    assert!(!m.capabilities.schema_graph);
    assert!(m.capabilities.read_only_exec && m.capabilities.params);
    assert!(m
        .connection_fields
        .iter()
        .any(|f| f.key == "password" && f.secret));
}
#[tokio::test]
async fn invalid_settings_do_not_expose_credentials() {
    for (name, value) in [
        ("host", "redis://:do-not-leak@127.0.0.1"),
        ("port", "do-not-leak"),
        ("db", "-1"),
    ] {
        let mut c = cfg();
        c.fields.insert(name.into(), value.into());
        let error = RedisDriver::new().connect(&c).await.err().unwrap();
        assert!(!format!("{error:?}").contains("do-not-leak"));
    }
}
