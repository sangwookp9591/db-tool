use dbcore::*;
use driver_mongodb::MongoDriver;
use mongodb::{
    bson::{
        doc, oid::ObjectId, spec::BinarySubtype, Binary, Bson, DateTime, Decimal128, Document,
        Regex, Timestamp,
    },
    Client,
};
use std::{collections::BTreeMap, str::FromStr, time::Duration};

tokio::task_local! { static FIXTURE_SUFFIX: String; }
fn fixture_name(name: &str) -> String {
    FIXTURE_SUFFIX.with(|suffix| format!("{name}_{suffix}"))
}
fn req(text: &str) -> ExecRequest {
    let text = text
        .replace("m1_types", &fixture_name("m1_types"))
        .replace("m1_pages", &fixture_name("m1_pages"))
        .replace("m1_forbidden", &fixture_name("m1_forbidden"));
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
async fn run(s: &dyn Session, text: &str) -> ResultSet {
    s.execute(&req(text), CancelToken::new())
        .await
        .unwrap()
        .remove(0)
}
fn cell<'a>(r: &'a ResultSet, name: &str) -> &'a Value {
    let t = r.table.as_ref().unwrap();
    &t.rows[0][t.columns.iter().position(|c| c.name == name).unwrap()]
}

#[tokio::test]
async fn real_mongodb_contract() {
    FIXTURE_SUFFIX
        .scope(ObjectId::new().to_hex(), real_contract())
        .await;
}
async fn real_contract() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        eprintln!("AXDB_TEST_MONGO unset: integration test not run");
        return;
    };
    assert_eq!(
        uri, "mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin",
        "test connects only to the isolated MongoDB"
    );
    let client = Client::with_uri_str(&uri).await.unwrap();
    let db = client.database("axdb_test");
    let types = db.collection::<Document>(&fixture_name("m1_types"));
    let oid = ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap();
    let decimal = "123456789012345678901234567890.1234";
    types.insert_one(doc! {
        "_id":oid,"i32":42_i32,"i64":i64::MAX,"double":1.5,"decimal":Decimal128::from_str(decimal).unwrap(),"string":"hello","bool":true,"null":Bson::Null,
        "date":DateTime::from_millis(1),"timestamp":Timestamp { time:12, increment:3 },
        "binary":Binary { subtype:BinarySubtype::Generic,bytes:vec![0,255] },
        "nested":doc!{"z":1,"a":true},"array":[Bson::Int32(1),Bson::Null],
        "regex":Regex { pattern:"^a".into(),options:"i".into() },"js":Bson::JavaScriptCode("return 1".into()),"min":Bson::MinKey,"max":Bson::MaxKey,
        "nan":f64::NAN,"infinity":f64::INFINITY,
    }).await.unwrap();
    let pages = db.collection::<Document>(&fixture_name("m1_pages"));
    pages
        .insert_many((0..240).map(|i| doc! {"_id":i,"n":i}))
        .await
        .unwrap();
    let cfg = ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), "mongodb://127.0.0.1:57017".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "axdb".into()),
            ("authSource".into(), "admin".into()),
        ]),
        secret: Some("testmongo".into()),
        connect_timeout_ms: 3000,
    };
    let driver = MongoDriver::new();
    assert_eq!(driver.manifest().capabilities.cancel, CancelSupport::Abort);
    let s = driver.connect(&cfg).await.unwrap();
    s.ping().await.unwrap();
    assert!(s.server_info().await.unwrap().starts_with("MongoDB "));
    let r = run(&*s, r#"{"find":"m1_types","filter":{}}"#).await;
    assert_eq!(r.row_count, 1);
    assert_eq!(r.documents.as_ref().unwrap().len(), 1);
    assert_eq!(
        cell(&r, "_id"),
        &Value::Other {
            type_name: "ObjectId".into(),
            text: oid.to_hex()
        }
    );
    assert_eq!(cell(&r, "i32"), &Value::Int("42".into()));
    assert_eq!(cell(&r, "i64"), &Value::Int(i64::MAX.to_string()));
    assert_eq!(cell(&r, "double"), &Value::Float(FloatValue::Number(1.5)));
    assert_eq!(cell(&r, "decimal"), &Value::Decimal(decimal.into()));
    assert_eq!(cell(&r, "string"), &Value::Text("hello".into()));
    assert_eq!(cell(&r, "bool"), &Value::Bool(true));
    assert_eq!(cell(&r, "null"), &Value::Null);
    assert_eq!(
        cell(&r, "date"),
        &Value::DateTimeTz("1970-01-01T00:00:00.001Z".into())
    );
    assert_eq!(
        cell(&r, "binary"),
        &Value::Bytes {
            len: 2,
            hex: "00ff".into()
        }
    );
    assert_eq!(
        cell(&r, "nested"),
        &Value::Map(vec![
            ("z".into(), Value::Int("1".into())),
            ("a".into(), Value::Bool(true))
        ])
    );
    assert_eq!(
        cell(&r, "array"),
        &Value::List(vec![Value::Int("1".into()), Value::Null])
    );
    for (name, kind) in [
        ("timestamp", "Timestamp"),
        ("regex", "Regex"),
        ("js", "JavaScript"),
        ("min", "MinKey"),
        ("max", "MaxKey"),
    ] {
        assert!(matches!(cell(&r,name),Value::Other{type_name,..} if type_name==kind));
    }
    assert_eq!(
        cell(&r, "nan"),
        &Value::Float(FloatValue::Special("NaN".into()))
    );
    assert_eq!(
        cell(&r, "infinity"),
        &Value::Float(FloatValue::Special("Infinity".into()))
    );
    let ejson = run(
        &*s,
        r#"{"find":"m1_types","filter":{"i64":{"$numberLong":"9223372036854775807"}}}"#,
    )
    .await;
    assert_eq!(ejson.row_count, 1);
    let mut limited = req(r#"{"find":"m1_pages","filter":{},"sort":{"_id":1}}"#);
    limited.row_limit = 120;
    let r = s
        .execute(&limited, CancelToken::new())
        .await
        .unwrap()
        .remove(0);
    assert_eq!(r.row_count, 120);
    assert!(r.truncated);
    assert_eq!(
        r.table.as_ref().unwrap().rows[119][0],
        Value::Int("119".into())
    );
    limited.row_limit = 240;
    let r = s
        .execute(&limited, CancelToken::new())
        .await
        .unwrap()
        .remove(0);
    assert!(!r.truncated);
    assert_eq!(r.row_count, 240);
    for text in ["db.m1_pages.find()", r#"{"find": "m1_pages", broken}"#] {
        let e = s.execute(&req(text), CancelToken::new()).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Query);
        assert!(e.message.contains("Extended JSON"));
    }
    let e = s
        .execute(&req(r#"{"nonexistentCommand":1}"#), CancelToken::new())
        .await
        .unwrap_err();
    assert!(e.detail.unwrap().contains("codeName=CommandNotFound"));
    let mut readonly = req(r#"{"insert":"m1_pages","documents":[{"_id":99999}]}"#);
    readonly.read_only = true;
    assert_eq!(
        s.execute(&readonly, CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Unsupported
    );
    assert_eq!(pages.count_documents(doc! {"_id":99999}).await.unwrap(), 0);
    for text in [
        r#"{"aggregate":"m1_pages","pipeline":[{"$out":"m1_forbidden"}],"cursor":{}}"#,
        r#"{"aggregate":"m1_pages","pipeline":[{"$lookup":{"from":"m1_pages","pipeline":[{"$merge":"m1_forbidden"}],"as":"x"}}],"cursor":{}}"#,
    ] {
        let mut r = req(text);
        r.read_only = true;
        assert_eq!(
            s.execute(&r, CancelToken::new()).await.unwrap_err().kind,
            ErrorKind::Unsupported
        );
    }
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert_eq!(
        s.execute(
            &req(r#"{"insert":"m1_pages","documents":[{"_id":99998}]}"#),
            cancelled
        )
        .await
        .unwrap_err()
        .kind,
        ErrorKind::Cancelled
    );
    assert_eq!(pages.count_documents(doc! {"_id":99998}).await.unwrap(), 0);
    let mut slow = req(
        r#"{"aggregate":"m1_pages","pipeline":[{"$limit":1},{"$project":{"a":{"$range":[0,2000]}}},{"$unwind":"$a"},{"$project":{"a":1,"b":{"$range":[0,2000]}}},{"$unwind":"$b"},{"$group":{"_id":null,"total":{"$sum":{"$add":["$a","$b"]}}}}],"cursor":{}}"#,
    );
    slow.timeout_ms = 60;
    assert_eq!(
        s.execute(&slow, CancelToken::new()).await.unwrap_err().kind,
        ErrorKind::Timeout
    );
    assert_eq!(run(&*s, r#"{"count":"m1_pages"}"#).await.row_count, 1);
    s.ping().await.unwrap();
    let mut server_limited = slow.clone();
    let mut command: serde_json::Value = serde_json::from_str(&server_limited.text).unwrap();
    command["maxTimeMS"] = 1.into();
    server_limited.text = command.to_string();
    server_limited.timeout_ms = 5000;
    let error = s
        .execute(&server_limited, CancelToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Timeout, "{error:?}");
    assert!(error.detail.unwrap().contains("codeName=MaxTimeMSExpired"));
    s.ping().await.unwrap();
    let token = CancelToken::new();
    let cancel = token.clone();
    slow.timeout_ms = 5000;
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(60)).await;
        cancel.cancel();
    });
    assert_eq!(
        s.execute(&slow, token).await.unwrap_err().kind,
        ErrorKind::Cancelled
    );
    s.ping().await.unwrap();
    let roots = s.list_children(None).await.unwrap();
    let database = roots.iter().find(|n| n.label == "axdb_test").unwrap();
    let collections = s.list_children(Some(&database.id)).await.unwrap();
    let collection = collections
        .iter()
        .find(|n| n.label == fixture_name("m1_pages"))
        .unwrap();
    assert_eq!(collection.actions.len(), 2);
    assert!(collection.detail.as_ref().unwrap().contains("240"));
    let folders = s.list_children(Some(&collection.id)).await.unwrap();
    let fields = s
        .list_children(Some(
            &folders.iter().find(|n| n.label == "Fields").unwrap().id,
        ))
        .await
        .unwrap();
    assert!(fields
        .iter()
        .any(|n| n.label == "n" && n.detail.as_deref() == Some("Int32")));
    let indexes = s
        .list_children(Some(
            &folders.iter().find(|n| n.label == "Indexes").unwrap().id,
        ))
        .await
        .unwrap();
    assert!(indexes.iter().any(|n| n.label == "_id_"));
    let page = s
        .browse(
            &collection.id,
            PageRequest {
                offset: 10,
                limit: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cell(&page, "_id"), &Value::Int("10".into()));
    assert_eq!(page.row_count, 5);
    assert!(page.page.unwrap().has_more);
    let last = s
        .browse(
            &collection.id,
            PageRequest {
                offset: 238,
                limit: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(last.row_count, 2);
    assert!(!last.page.unwrap().has_more);
    let distinct = run(&*s, r#"{"distinct":"m1_pages","key":"n"}"#).await;
    assert_eq!(distinct.row_count, 240);
    let aggregate = run(
        &*s,
        r#"{"aggregate":"m1_pages","pipeline":[{"$match":{"n":{"$lt":3}}}],"cursor":{}}"#,
    )
    .await;
    assert_eq!(aggregate.row_count, 3);
    let write = run(&*s, r#"{"insert":"m1_pages","documents":[{"_id":99999}]}"#).await;
    assert_eq!(write.affected, Some(1));
    assert!(write.message.is_some());
    assert_eq!(
        s.execute(
            &req(r#"{"insert":"m1_pages","documents":[{"_id":99999}]}"#),
            CancelToken::new()
        )
        .await
        .unwrap_err()
        .kind,
        ErrorKind::Query
    );
    pages.delete_one(doc! {"_id":99999}).await.unwrap();
    let mut bad = cfg.clone();
    bad.secret = Some("m1-wrong-password".into());
    let error = driver.connect(&bad).await.err().unwrap();
    assert_eq!(error.kind, ErrorKind::Auth);
    assert!(!error.message.contains("m1-wrong-password"));
    bad = cfg.clone();
    bad.fields
        .insert("uri".into(), "mongodb://127.0.0.1:57018".into());
    bad.connect_timeout_ms = 200;
    assert_eq!(
        driver.connect(&bad).await.err().unwrap().kind,
        ErrorKind::Connection
    );
    types.drop().await.unwrap();
    pages.drop().await.unwrap();
    s.close().await;
    assert_eq!(s.ping().await.unwrap_err().kind, ErrorKind::Connection);
}
