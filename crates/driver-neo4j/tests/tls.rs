use dbcore::*;
use driver_neo4j::Neo4jDriver;
use std::collections::BTreeMap;

#[tokio::test]
async fn real_neo4j_tls_modes() {
    let Ok(connection) = std::env::var("AXDB_TEST_NEO4J_TLS") else {
        eprintln!("AXDB_TEST_NEO4J_TLS unset: TLS integration test not run");
        return;
    };
    assert_eq!(connection, "bolt://localhost:57688|neo4j|testpassword");
    let ca = std::env::var("AXDB_TEST_TLS_CA").expect("TLS fixture CA");
    let wrong_ca = std::env::var("AXDB_TEST_TLS_WRONG_CA").expect("unrelated TLS fixture CA");
    for (scheme, host, root, succeeds) in [
        ("bolt", "localhost", None, false),
        ("bolt+ssc", "localhost", None, true),
        ("bolt+s", "localhost", None, false),
        ("bolt+s", "localhost", Some(ca.as_str()), true),
        ("bolt+s", "127.0.0.1", Some(ca.as_str()), false),
        ("bolt+s", "localhost", Some(wrong_ca.as_str()), false),
        ("bolt+ssc", "localhost", Some(ca.as_str()), false),
    ] {
        let mut cfg = ConnectConfig {
            fields: BTreeMap::from([
                ("uri".into(), format!("{scheme}://{host}:57688")),
                ("user".into(), "neo4j".into()),
            ]),
            secret: Some("testpassword".into()),
            connect_timeout_ms: 3000,
        };
        if let Some(root) = root {
            cfg.fields.insert("tlsCAFile".into(), root.into());
        }
        let result = Neo4jDriver::new().connect(&cfg).await;
        assert_eq!(
            result.is_ok(),
            succeeds,
            "scheme={scheme} host={host} root={root:?}: {:?}",
            result.as_ref().err()
        );
        if let Ok(session) = result {
            session.ping().await.unwrap();
            session.close().await;
        }
    }
}
