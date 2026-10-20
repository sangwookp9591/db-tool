use dbcore::*;
use driver_mongodb::MongoDriver;
use std::collections::BTreeMap;
#[tokio::test]
async fn explain_reads_only_and_exposes_execution_stats() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        return;
    };
    let test_database=format!("axdbt_d10_explain_{}",mongodb::bson::oid::ObjectId::new().to_hex());
    let options = mongodb::options::ClientOptions::parse(uri).await.unwrap();
    assert_eq!(
        options.hosts,
        vec![mongodb::options::ServerAddress::Tcp {
            host: "127.0.0.1".into(),
            port: Some(57017)
        }]
    );
    let s = MongoDriver::new()
        .connect(&ConnectConfig {
            fields: BTreeMap::from([
                ("uri".into(), "mongodb://127.0.0.1:57017".into()),
                ("user".into(), "axdb".into()),
                ("authSource".into(), "admin".into()),
                ("database".into(), test_database.as_str().into()),
            ]),
            secret: options.credential.and_then(|c| c.password),
            connect_timeout_ms: 5000,
        })
        .await
        .unwrap();
    let p = s
        .explain(
            r#"{"find":"w2_explain_missing","filter":{}}"#,
            false,
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert!(!p.op.is_empty());
    s.explain(
        r#"{"find":"w2_explain_missing","filter":{}}"#,
        true,
        CancelToken::new(),
    )
    .await
    .unwrap();
    let aggregate = s.explain(
        r#"{"aggregate":"w2_explain_missing","pipeline":[{"$facet":{"all":[{"$match":{}}]}}],"cursor":{}}"#,
        true,
        CancelToken::new(),
    ).await.unwrap();
    assert_eq!(aggregate.op, "Aggregation");
    assert!(aggregate.children.iter().any(|p| p.op == "$facet"));
    assert!(s
        .explain(
            r#"{"delete":"w2_explain_missing","deletes":[{"q":{},"limit":0}]}"#,
            true,
            CancelToken::new()
        )
        .await
        .is_err());
    assert!(s
        .explain(
            r#"{"aggregate":"x","pipeline":[{"$out":"target"}],"cursor":{}}"#,
            true,
            CancelToken::new()
        )
        .await
        .is_err());
    s.close().await;
}
