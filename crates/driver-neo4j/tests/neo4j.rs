// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_neo4j::Neo4jDriver;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
fn config() -> Option<ConnectConfig> {
    let value = std::env::var("AXDB_TEST_NEO4J").ok()?;
    let parts = value.split('|').collect::<Vec<_>>();
    assert_eq!(parts.len(), 3, "test URI|user|password");
    assert_eq!(
        parts[0], "bolt://127.0.0.1:57687",
        "test connects only to isolated Neo4j"
    );
    Some(ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), parts[0].into()),
            ("user".into(), parts[1].into()),
        ]),
        secret: Some(parts[2].into()),
        connect_timeout_ms: 3000,
    })
}
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
async fn run(s: &dyn Session, text: &str) -> ResultSet {
    s.execute(&req(text), CancelToken::new())
        .await
        .unwrap()
        .remove(0)
}
fn cell<'a>(r: &'a ResultSet, key: &str) -> &'a Value {
    let t = r.table.as_ref().unwrap();
    &t.rows[0][t.columns.iter().position(|c| c.name == key).unwrap()]
}
#[tokio::test]
async fn real_neo4j_contract() {
    let Some(cfg) = config() else {
        eprintln!("AXDB_TEST_NEO4J unset: integration test not run");
        return;
    };
    let driver = Neo4jDriver::new();
    let s = driver.connect(&cfg).await.unwrap();
    s.ping().await.unwrap();
    for (text, expected, count) in [
        ("RETURN 1 AS z, 2 AS a, 3 AS m", vec!["z", "a", "m"], 1),
        (
            "MATCH (n:NoSuchLabel) RETURN n.x AS x, n.y AS y",
            vec!["x", "y"],
            0,
        ),
    ] {
        let result = run(&*s, text).await;
        let table = result.table.unwrap();
        assert_eq!(
            table
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(table.rows.len(), count);
    }

    let server_info = s.server_info().await.unwrap();
    assert!(server_info.starts_with("Neo4j 5."), "{server_info}");
    assert!(!server_info.contains("[[Text("), "{server_info}");
    let mut parameter = req("RETURN $n AS n, $map AS m, $list AS l");
    parameter.params=serde_json::json!({"n":{"t":"Int","v":"9007199254740993"},"map":{"a":true},"list":[1,null,"x"]}).as_object().unwrap().clone();
    let result = s
        .execute(&parameter, CancelToken::new())
        .await
        .unwrap()
        .remove(0);
    assert_eq!(cell(&result, "n"), &Value::Int("9007199254740993".into()));
    assert!(matches!(cell(&result, "m"), Value::Map(_)));
    assert!(matches!(cell(&result, "l"), Value::List(_)));
    assert!(result.graph.is_none());
    let temporal=run(&*s,"RETURN date('2026-10-15') AS d, localtime('12:34:56.123456789') AS lt, time('12:34:56.123456789+09:00') AS t, localdatetime('2026-10-15T12:34:56.123456789') AS ldt, datetime('2026-10-15T12:34:56.123456789+09:00') AS dt, datetime('2026-10-15T12:34:56+09:00[Asia/Seoul]') AS zdt, duration('P2M3DT4.000000005S') AS dur, point({x:1,y:2}) AS p2, point({x:1,y:2,z:3}) AS p3, 9007199254740993 AS big, null AS nil, true AS b, 1.5 AS f").await;
    assert_eq!(cell(&temporal, "d"), &Value::Date("2026-10-15".into()));
    assert_eq!(
        cell(&temporal, "lt"),
        &Value::Time("12:34:56.123456789".into())
    );
    assert_eq!(
        cell(&temporal, "t"),
        &Value::Time("12:34:56.123456789+09:00".into())
    );
    assert_eq!(
        cell(&temporal, "ldt"),
        &Value::DateTime("2026-10-15T12:34:56.123456789".into())
    );
    assert_eq!(
        cell(&temporal, "dt"),
        &Value::DateTimeTz("2026-10-15T12:34:56.123456789+09:00".into())
    );
    assert_eq!(
        cell(&temporal, "zdt"),
        &Value::DateTimeTz("2026-10-15T12:34:56+09:00[Asia/Seoul]".into())
    );
    assert_eq!(
        cell(&temporal, "dur"),
        &Value::Duration("P2M3DT4.000000005S".into())
    );
    assert!(matches!(
        cell(&temporal, "p2"),
        Value::Point { z: None, .. }
    ));
    assert!(matches!(
        cell(&temporal, "p3"),
        Value::Point { z: Some(3.0), .. }
    ));
    assert_eq!(
        cell(&temporal, "big"),
        &Value::Int("9007199254740993".into())
    );
    assert_eq!(cell(&temporal, "nil"), &Value::Null);
    let nullable = run(&*s, "UNWIND [null, 7] AS value RETURN value").await;
    assert_eq!(
        nullable.table.as_ref().unwrap().columns[0].type_name,
        "Integer"
    );
    let label = format!("DbtT_W2_{}", std::process::id());
    run(&*s, &format!("MATCH (n:`{label}`) DETACH DELETE n")).await;
    let created=run(&*s,&format!("CREATE (a:`{label}` {{i:1}})-[:DbtT_LINK]->(b:`{label}` {{i:2}}), (c:`{label}` {{i:3}}) RETURN a")).await;
    assert!(created
        .message
        .as_ref()
        .unwrap()
        .contains("nodes created: 3"));
    assert!(created.affected.unwrap() >= 4);
    let unchanged = run(
        &*s,
        &format!("MATCH (n:`{label}`) WHERE n.i = 999 DELETE n"),
    )
    .await;
    assert_eq!(unchanged.affected, Some(0));
    assert_eq!(unchanged.message.as_deref(), Some("No updates"));
    let sql = format!(
        "MATCH p=(n:`{label}`)-[r:DbtT_LINK]->(m) RETURN n, r, p, [n,m] AS list, {{a:n}} AS map"
    );
    let mixed = run(&*s, &sql).await;
    assert_eq!(mixed.table.as_ref().unwrap().columns.len(), 5);
    assert_eq!(mixed.graph.as_ref().unwrap().nodes.len(), 2);
    assert_eq!(mixed.graph.as_ref().unwrap().rels.len(), 1);
    assert_eq!(
        mixed
            .table
            .as_ref()
            .unwrap()
            .columns
            .iter()
            .find(|c| c.name == "p")
            .unwrap()
            .type_name,
        "Path"
    );
    let reverse = run(
        &*s,
        &format!("MATCH p=(m:`{label}`)<-[:DbtT_LINK]-(n:`{label}`) RETURN p"),
    )
    .await;
    let original = mixed.graph.as_ref().unwrap();
    assert_eq!(reverse.graph.as_ref().unwrap().rels, original.rels);
    let id = mixed.graph.as_ref().unwrap().nodes[0].id.clone();
    let expanded = s.expand(&id, 25).await.unwrap();
    assert_eq!(expanded.graph.unwrap().nodes.len(), 2);
    assert!(expanded.table.is_some());
    let cap = s
        .execute(
            &ExecRequest {
                graph_node_limit: 1,
                ..req(&sql)
            },
            CancelToken::new(),
        )
        .await
        .unwrap()
        .remove(0);
    assert!(cap.truncated);
    assert_eq!(cap.graph.as_ref().unwrap().nodes.len(), 1);
    assert!(cap.graph.unwrap().rels.is_empty());
    let cap = s
        .execute(
            &ExecRequest {
                row_limit: 2,
                ..req("UNWIND range(1,10000) AS x RETURN x")
            },
            CancelToken::new(),
        )
        .await
        .unwrap()
        .remove(0);
    assert_eq!(cap.row_count, 2);
    assert!(cap.truncated);
    assert_eq!(
        cell(&run(&*s, "RETURN 1 AS one").await, "one"),
        &Value::Int("1".into())
    );
    let err = s
        .execute(&req("THIS IS NOT CYPHER"), CancelToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Query);
    assert!(err.detail.unwrap().contains("SyntaxError"));
    let long = "UNWIND range(1, 100000000) AS x WITH x WHERE x < 0 RETURN x";
    let start = Instant::now();
    let err = s
        .execute(
            &ExecRequest {
                timeout_ms: 500,
                ..req(long)
            },
            CancelToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    assert!(start.elapsed() < Duration::from_secs(3));
    run(&*s, "RETURN 1").await;
    let token = CancelToken::new();
    let trigger = token.clone();
    let stop = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        trigger.cancel();
    });
    let start = Instant::now();
    let err = s.execute(&req(long), token).await.unwrap_err();
    stop.await.unwrap();
    assert_eq!(err.kind, ErrorKind::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(3));
    run(&*s, "RETURN 1").await;
    let roots = s.list_children(None).await.unwrap();
    assert_eq!(roots.len(), 4);
    let labels = s
        .list_children(Some(
            &roots.iter().find(|n| n.label == "레이블").unwrap().id,
        ))
        .await
        .unwrap();
    let label_node = labels.iter().find(|n| n.label == label).unwrap();
    assert_eq!(label_node.actions.len(), 2);
    let props = s.list_children(Some(&label_node.id)).await.unwrap();
    assert!(props.iter().any(|n| n.label == "i"
        && n.detail
            .as_ref()
            .is_some_and(|v| v.contains("Integer") || v.contains("Long"))));
    for root in roots.iter().filter(|n| n.label != "레이블") {
        let children = s.list_children(Some(&root.id)).await.unwrap();
        if root.label == "관계 유형" {
            let rel = children.iter().find(|n| n.label == "DbtT_LINK").unwrap();
            let result = s
                .browse(
                    &rel.id,
                    PageRequest { filter: vec![],
                        offset: 0,
                        limit: 2,
                    },
                )
                .await
                .unwrap();
            assert!(result.graph.is_some());
        }
        if root.label == "속성 키" {
            assert!(children.iter().any(|n| n.label == "i"));
        }
    }
    let first = s
        .browse(
            &label_node.id,
            PageRequest { filter: vec![],
                offset: 0,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.row_count, 2);
    assert!(first.page.unwrap().has_more);
    assert_eq!(first.graph.unwrap().nodes.len(), 2);
    let last = s
        .browse(
            &label_node.id,
            PageRequest { filter: vec![],
                offset: 2,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(last.row_count, 1);
    assert!(!last.page.unwrap().has_more);
    let maximum = s
        .browse(
            &label_node.id,
            PageRequest { filter: vec![],
                offset: 0,
                limit: 100_000,
            },
        )
        .await
        .unwrap();
    assert_eq!(maximum.page.as_ref().unwrap().limit, 100_000);
    assert!(!maximum.page.unwrap().has_more);
    let readonly = ExecRequest {
        read_only: true,
        ..req(&format!("CREATE (:`{label}`)"))
    };
    let error = s.execute(&readonly, CancelToken::new()).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Query);
    assert!(error
        .detail
        .as_deref()
        .is_some_and(|code| code.contains("AccessMode") || code.contains("Forbidden")));
    let readonly = ExecRequest {
        read_only: true,
        ..req("RETURN 1 AS one")
    };
    assert_eq!(
        s.execute(&readonly, CancelToken::new()).await.unwrap()[0].row_count,
        1
    );
    run(&*s, &format!("MATCH (n:`{label}`) DETACH DELETE n")).await;
    s.close().await;
    assert_eq!(s.ping().await.unwrap_err().kind, ErrorKind::Connection);
    let mut bad = cfg.clone();
    bad.secret = Some("axdb-wrong-password".into());
    let err = driver.connect(&bad).await.err().unwrap();
    assert_eq!(err.kind, ErrorKind::Auth);
    assert!(!err.message.contains("axdb-wrong-password"));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    bad.fields
        .insert("uri".into(), format!("bolt://127.0.0.1:{port}"));
    assert_eq!(
        driver.connect(&bad).await.err().unwrap().kind,
        ErrorKind::Connection
    );
}

#[tokio::test]
async fn change_preview_rolls_back_graph_writes_and_interruptions() {
    let Some(cfg) = config() else {
        return;
    };
    let s = Neo4jDriver::new().connect(&cfg).await.unwrap();
    let marker = format!(
        "preview_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    run(
        &*s,
        &format!("CREATE (:AxdbPreview {{marker:'{marker}', n:'before'}})"),
    )
    .await;
    for (sql,counter) in [
        (format!("CREATE (n:AxdbPreview {{marker:'{marker}',n:'created'}}) RETURN n"),"nodes_created"),
        (format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) SET n.n='after' RETURN n"),"properties_set"),
        (format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) DELETE n"),"nodes_deleted"),
        (format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) CREATE (n)-[r:PREVIEW]->(m:AxdbPreview {{marker:'{marker}'}}) RETURN n,r,m"),"relationships_created"),
    ] {
        let mut r=req(&sql);r.preview_sample_limit=1;
        let p=s.preview(&r,CancelToken::new()).await.unwrap();assert!(!p.committed);
        assert!(p.statements[0].counters.iter().any(|(k,n)| k==counter&&*n>0));
        if sql.contains("RETURN") {assert_eq!(p.statements[0].after.as_ref().unwrap().rows.len(),1);}
        let actual=run(&*s,&format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) RETURN n.n AS n")).await;
        assert_eq!(actual.row_count,1);assert_eq!(cell(&actual,"n"),&Value::Text("before".into()));
        assert_eq!(cell(&run(&*s,&format!("MATCH (:AxdbPreview {{marker:'{marker}'}})-[r:PREVIEW]->() RETURN count(r) AS c")).await,"c"),&Value::Int("0".into()));
    }
    assert!(!s
        .preview(&req("CALL dbms.components()"), CancelToken::new())
        .await
        .unwrap()
        .notes
        .is_empty());
    for timeout in [true, false] {
        let mut r=req(&format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) SET n.n='bad' WITH n UNWIND range(1,100000000) AS x RETURN sum(x)"));
        let token = CancelToken::new();
        let other = token.clone();
        if timeout {
            r.timeout_ms = 50;
        } else {
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                other.cancel();
            });
        }
        assert_eq!(
            s.preview(&r, token).await.unwrap_err().kind,
            if timeout {
                ErrorKind::Timeout
            } else {
                ErrorKind::Cancelled
            }
        );
        let actual = run(
            &*s,
            &format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) RETURN n.n AS n"),
        )
        .await;
        assert_eq!(cell(&actual, "n"), &Value::Text("before".into()));
    }
    run(
        &*s,
        &format!("MATCH (n:AxdbPreview {{marker:'{marker}'}}) DETACH DELETE n"),
    )
    .await;
}

#[tokio::test]
async fn schema_graph_and_filtered_browse() {
    let Some(cfg)=config() else {eprintln!("AXDB_TEST_NEO4J unset: V1 integration test not run");return;};
    let s=Neo4jDriver::new().connect(&cfg).await.unwrap();
    run(&*s,"MATCH (n:DbtE_V1) DETACH DELETE n").await;
    run(&*s,"CREATE (:DbtE_V1 {id:9223372036854775807,name:'exact'})-[:DbtE_V1_LINK]->(:DbtE_V1 {id:1,name:'other'}),(:DbtE_V1:DbtE_V1_Mixed {id:'text',name:'mixed'})").await;
    let graph=s.schema_graph(None).await.unwrap();let e=graph.entities.iter().find(|e|e.name=="DbtE_V1").unwrap();assert!(e.fields.iter().any(|f|f.name=="id"&&f.type_name.contains("String")));assert!(graph.relations.iter().any(|r|r.name=="DbtE_V1_LINK"));
    let result=s.browse(&e.id,PageRequest {offset:0,limit:10,filter:vec![("id".into(),Value::Int(i64::MAX.to_string()))]}).await.unwrap();assert_eq!(result.row_count,1);let Value::Node(n)=&result.table.unwrap().rows[0][0] else {panic!()};assert_eq!(n.props.iter().find(|(k,_)|k=="id").unwrap().1,Value::Int(i64::MAX.to_string()));
    let result=s.browse(&e.id,PageRequest {offset:0,limit:10,filter:vec![("name".into(),Value::Text("exact' OR true //".into()))]}).await.unwrap();assert_eq!(result.row_count,0);
    s.close().await;
}
