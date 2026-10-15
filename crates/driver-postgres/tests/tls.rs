use dbcore::*;
use driver_postgres::PostgresDriver;
use std::collections::BTreeMap;

#[tokio::test]
async fn real_postgres_tls_modes() {
    let Ok(dsn) = std::env::var("AXDB_TEST_PG_TLS") else {
        eprintln!("AXDB_TEST_PG_TLS unset: TLS integration test not run");
        return;
    };
    let pg: tokio_postgres::Config = dsn.parse().unwrap();
    assert_eq!(
        pg.get_hosts(),
        &[tokio_postgres::config::Host::Tcp("localhost".into())]
    );
    assert_eq!(pg.get_ports(), &[55433]);
    let ca = std::env::var("AXDB_TEST_TLS_CA").expect("TLS fixture CA");
    let wrong_ca = std::env::var("AXDB_TEST_TLS_WRONG_CA").expect("unrelated TLS fixture CA");
    let base = ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "localhost".into()),
            ("port".into(), "55433".into()),
            ("database".into(), pg.get_dbname().unwrap().into()),
            ("user".into(), pg.get_user().unwrap().into()),
        ]),
        secret: pg
            .get_password()
            .map(|s| String::from_utf8(s.to_vec()).unwrap()),
        connect_timeout_ms: 3000,
    };
    // The fixture certificate has DNS:localhost only, deliberately excluding 127.0.0.1.
    for (mode, host, root, succeeds) in [
        ("require", "localhost", None, true),
        ("require", "127.0.0.1", None, true),
        ("require", "localhost", Some(ca.as_str()), true),
        ("require", "127.0.0.1", Some(ca.as_str()), true),
        ("prefer", "localhost", None, true),
        ("disable", "localhost", None, false),
        ("verify-ca", "localhost", Some(ca.as_str()), true),
        ("verify-ca", "127.0.0.1", Some(ca.as_str()), true),
        ("verify-full", "localhost", Some(ca.as_str()), true),
        ("verify-full", "127.0.0.1", Some(ca.as_str()), false),
        ("verify-ca", "localhost", Some(wrong_ca.as_str()), false),
        ("verify-full", "localhost", Some(wrong_ca.as_str()), false),
        ("verify-ca", "localhost", None, false),
        ("verify-full", "localhost", None, false),
        ("require", "localhost", Some(wrong_ca.as_str()), false),
    ] {
        let mut cfg = base.clone();
        cfg.fields.insert("sslmode".into(), mode.into());
        cfg.fields.insert("host".into(), host.into());
        if let Some(root) = root {
            cfg.fields.insert("sslrootcert".into(), root.into());
        }
        let result = PostgresDriver::new().connect(&cfg).await;
        assert_eq!(
            result.is_ok(),
            succeeds,
            "mode={mode} host={host} root={root:?}: {:?}",
            result.as_ref().err()
        );
        if let Ok(s) = result {
            let request = serde_json::from_value(serde_json::json!({"text":"SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()"})).unwrap();
            let r = s.execute(&request, CancelToken::new()).await.unwrap();
            assert_eq!(r[0].table.as_ref().unwrap().rows[0][0], Value::Bool(true));
            s.close().await;
        }
    }
}
