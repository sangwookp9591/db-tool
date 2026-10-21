// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_mongodb::MongoDriver;
use std::collections::BTreeMap;

#[tokio::test]
async fn real_mongodb_tls_modes() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO_TLS") else {
        eprintln!("AXDB_TEST_MONGO_TLS unset: TLS integration test not run");
        return;
    };
    assert_eq!(uri, "mongodb://localhost:57018");
    let ca = std::env::var("AXDB_TEST_TLS_CA").expect("TLS fixture CA");
    let wrong_ca = std::env::var("AXDB_TEST_TLS_WRONG_CA").expect("unrelated TLS fixture CA");
    for (host, tls, root, suffix, succeeds) in [
        ("localhost", "true", None, "", false),
        ("localhost", "false", None, "", false),
        ("localhost", "true", Some(ca.as_str()), "", true),
        ("localhost", "uri", Some(ca.as_str()), "", true),
        ("127.0.0.1", "true", Some(ca.as_str()), "", false),
        ("localhost", "true", Some(wrong_ca.as_str()), "", false),
        ("localhost", "false", Some(ca.as_str()), "", false),
        (
            "localhost",
            "uri",
            None,
            "/?tls=true&tlsAllowInvalidCertificates=true",
            true,
        ),
        ("localhost", "uri", Some(ca.as_str()), "/?tls=true", true),
        ("localhost", "uri", None, "/?tls=false", false),
    ] {
        let mut cfg = ConnectConfig {
            fields: BTreeMap::from([
                ("uri".into(), format!("mongodb://{host}:57018{suffix}")),
                ("database".into(), "axdb_test".into()),
                ("tls".into(), tls.into()),
            ]),
            secret: None,
            connect_timeout_ms: 1500,
        };
        if let Some(root) = root {
            cfg.fields.insert("tlsCAFile".into(), root.into());
        }
        let result = MongoDriver::new().connect(&cfg).await;
        assert_eq!(
            result.is_ok(),
            succeeds,
            "host={host} tls={tls} root={root:?} suffix={suffix}: {:?}",
            result.as_ref().err()
        );
        if let Ok(session) = result {
            session.ping().await.unwrap();
            session.close().await;
        }
    }
    // The native URI option also works without the added form fields.
    let cfg = ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), format!("{uri}/?tls=true&tlsCAFile={ca}")),
            ("database".into(), "axdb_test".into()),
        ]),
        secret: None,
        connect_timeout_ms: 1500,
    };
    let session = MongoDriver::new().connect(&cfg).await.unwrap();
    session.ping().await.unwrap();
    session.close().await;
}
