use dbcore::*;
use driver_sqlite::SqliteDriver;
use std::{collections::BTreeMap, time::Duration};
fn config(path: &std::path::Path, readonly: bool) -> ConnectConfig {
    ConnectConfig {
        fields: BTreeMap::from([
            ("path".into(), path.to_str().unwrap().into()),
            ("read_only_open".into(), readonly.to_string()),
        ]),
        secret: None,
        connect_timeout_ms: 1000,
    }
}
fn req(sql: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":sql})).unwrap()
}
async fn run(s: &dyn Session, sql: &str) -> ResultSet {
    s.execute(&req(sql), CancelToken::new())
        .await
        .unwrap()
        .remove(0)
}
async fn fixture() -> (tempfile::TempDir, Box<dyn Session>) {
    let dir = tempfile::tempdir().unwrap();
    let s = SqliteDriver::new()
        .connect(&config(&dir.path().join("test.db"), false))
        .await
        .unwrap();
    s.execute(&req("CREATE TABLE sample(id INTEGER PRIMARY KEY, n REAL, t TEXT, b BLOB, j JSON, d DATE, dt DATETIME, ts TIMESTAMP); CREATE INDEX sample_t ON sample(t); CREATE VIEW sample_view AS SELECT * FROM sample; INSERT INTO sample VALUES(9223372036854775807,1.5,NULL,x'00ff','{\"n\":9223372036854775807}','2026-10-15','2026-10-15 12:00:00','2026-10-15T12:00:00');"), CancelToken::new()).await.unwrap();
    (dir, s)
}
#[tokio::test]
async fn values_limits_errors_and_explorer() {
    let (_dir, s) = fixture().await;
    s.ping().await.unwrap();
    assert!(!s.server_info().await.unwrap().is_empty());
    let r = run(s.as_ref(), "SELECT * FROM sample").await;
    let table = r.table.unwrap();
    assert_eq!(table.columns[4].type_name, "JSON");
    assert_eq!(
        table.rows[0],
        vec![
            Value::Int(i64::MAX.to_string()),
            Value::Float(FloatValue::Number(1.5)),
            Value::Null,
            Value::Bytes {
                len: 2,
                hex: "00ff".into()
            },
            Value::Json("{\"n\":9223372036854775807}".into()),
            Value::Date("2026-10-15".into()),
            Value::DateTime("2026-10-15 12:00:00".into()),
            Value::DateTime("2026-10-15T12:00:00".into())
        ]
    );
    let expr = run(
        s.as_ref(),
        "SELECT 1 AS n, NULL AS nothing_here, 'text' AS t",
    )
    .await;
    assert_eq!(
        expr.table
            .unwrap()
            .columns
            .iter()
            .map(|c| c.type_name.as_str())
            .collect::<Vec<_>>(),
        ["INTEGER", "NULL", "TEXT"]
    );
    let mut limited = req("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000) SELECT x FROM n");
    limited.row_limit = 2;
    let r = s
        .execute(&limited, CancelToken::new())
        .await
        .unwrap()
        .remove(0);
    assert_eq!(r.row_count, 2);
    assert!(r.truncated);
    assert_eq!(
        s.execute(&req("SELEC broken"), CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Query
    );
    assert_eq!(s.list_children(None).await.unwrap()[0].label, "테이블");
    let tables = s.list_children(Some("tables")).await.unwrap();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].actions.len(), 2);
    assert_eq!(
        s.list_children(Some("views")).await.unwrap()[0].label,
        "sample_view"
    );
    assert_eq!(
        s.list_children(Some("table:sample")).await.unwrap()[0].label,
        "컬럼"
    );
    let cols = s.list_children(Some("columns:sample")).await.unwrap();
    assert_eq!(cols[0].icon, Icon::Key);
    assert!(cols[0].detail.as_ref().unwrap().contains("PRIMARY KEY"));
    assert!(s.list_children(Some("indexes:sample")).await.unwrap()[0]
        .detail
        .as_ref()
        .unwrap()
        .contains("CREATE INDEX"));
    run(
        s.as_ref(),
        "CREATE TABLE \"weird\"\";name\"(n); INSERT INTO \"weird\"\";name\" VALUES(1),(2),(3)",
    )
    .await;
    let p = s
        .browse(
            "table:weird\";name",
            PageRequest { filter: vec![],
                offset: 0,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert!(p.page.unwrap().has_more);
    assert_eq!(p.row_count, 2);
    let p = s
        .browse(
            "table:weird\";name",
            PageRequest { filter: vec![],
                offset: 2,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert!(!p.page.unwrap().has_more);
    assert_eq!(p.row_count, 1);
    s.close().await;
    assert_eq!(s.ping().await.unwrap_err().kind, ErrorKind::Connection);
}
const LOOP: &str =
    "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n) SELECT sum(x) FROM n";
#[tokio::test]
async fn timeout_cancel_and_session_reuse() {
    let (_dir, s) = fixture().await;
    let mut r = req(LOOP);
    r.timeout_ms = 300;
    let start = std::time::Instant::now();
    assert_eq!(
        s.execute(&r, CancelToken::new()).await.unwrap_err().kind,
        ErrorKind::Timeout
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(run(s.as_ref(), "SELECT 1").await.row_count, 1);
    let token = CancelToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    assert_eq!(
        s.execute(&req(LOOP), token).await.unwrap_err().kind,
        ErrorKind::Cancelled
    );
    assert_eq!(run(s.as_ref(), "SELECT 1").await.row_count, 1);
    // Dropping a caller must not leave an infinite blocking task holding the connection.
    let mut r = req(LOOP);
    r.timeout_ms = 100;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), s.execute(&r, CancelToken::new()))
            .await
            .is_err()
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), run(s.as_ref(), "SELECT 1"))
            .await
            .unwrap()
            .row_count,
        1
    );
}
#[tokio::test]
async fn readonly_cannot_be_disabled_and_is_restored() {
    let (dir, s) = fixture().await;
    for sql in [
        "INSERT INTO sample(id) VALUES(1)",
        "PRAGMA query_only=OFF; INSERT INTO sample(id) VALUES(1)",
        "ATTACH ':memory:' AS extra",
        "BEGIN",
        "SAVEPOINT test",
    ] {
        let mut r = req(sql);
        r.read_only = true;
        assert_eq!(
            s.execute(&r, CancelToken::new()).await.unwrap_err().kind,
            ErrorKind::Query,
            "{sql}"
        );
        assert_eq!(run(s.as_ref(), "SELECT 1").await.row_count, 1);
    }
    let mut r = req("SELECT * FROM sample");
    r.read_only = true;
    assert_eq!(
        s.execute(&r, CancelToken::new()).await.unwrap()[0].row_count,
        1
    );
    run(s.as_ref(), "INSERT INTO sample(id) VALUES(1)").await;
    run(
        s.as_ref(),
        "BEGIN; INSERT INTO sample(id) VALUES(2); ROLLBACK;",
    )
    .await;
    assert_eq!(run(s.as_ref(), "SELECT * FROM sample").await.row_count, 2);
    let ro = SqliteDriver::new()
        .connect(&config(&dir.path().join("test.db"), true))
        .await
        .unwrap();
    assert_eq!(
        ro.execute(&req("INSERT INTO sample(id) VALUES(3)"), CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Query
    );
    assert!(matches!(
        SqliteDriver::new()
            .connect(&config(&dir.path().join("missing.db"), true))
            .await,
        Err(DbError {
            kind: ErrorKind::Connection,
            ..
        })
    ));
}

#[tokio::test]
async fn parser_and_readonly_cleanup_after_timeout() {
    let (_dir, s) = fixture().await;
    let result = s.execute(&req("CREATE TABLE audit(v TEXT); CREATE TRIGGER sample_t AFTER INSERT ON sample BEGIN INSERT INTO audit VALUES('one;two'); INSERT INTO audit VALUES('three'); END; INSERT INTO sample(id) VALUES(5); SELECT * FROM audit;"), CancelToken::new()).await.unwrap();
    assert_eq!(result.len(), 4);
    let indexes = s.list_children(Some("indexes:sample")).await.unwrap();
    assert_eq!(indexes.len(), 1);
    assert!(indexes[0]
        .detail
        .as_ref()
        .unwrap()
        .starts_with("CREATE INDEX"));
    assert_eq!(result[3].row_count, 2);
    assert_eq!(
        result[3].table.as_ref().unwrap().rows[0][0],
        Value::Text("one;two".into())
    );
    let mut r = req(LOOP);
    r.read_only = true;
    r.timeout_ms = 50;
    assert_eq!(
        s.execute(&r, CancelToken::new()).await.unwrap_err().kind,
        ErrorKind::Timeout
    );
    run(s.as_ref(), "INSERT INTO sample(id) VALUES(6)").await;
    run(s.as_ref(), "PRAGMA query_only=ON").await;
    let mut r = req("SELECT 1");
    r.read_only = true;
    s.execute(&r, CancelToken::new()).await.unwrap();
    assert_eq!(
        s.execute(&req("INSERT INTO sample(id) VALUES(7)"), CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Query
    );
    run(s.as_ref(), "PRAGMA query_only=OFF").await;
    run(s.as_ref(), "INSERT INTO sample(id) VALUES(7)").await;
}

#[tokio::test]
async fn queued_timeout_and_precancel_do_not_interrupt_next_run() {
    let (_dir, s) = fixture().await;
    let mut long = req(LOOP);
    long.timeout_ms = 200;
    let waiting = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut short = req("SELECT 1");
        short.timeout_ms = 20;
        s.execute(&short, CancelToken::new()).await
    };
    let (first, queued) = tokio::join!(s.execute(&long, CancelToken::new()), waiting);
    assert_eq!(first.unwrap_err().kind, ErrorKind::Timeout);
    assert_eq!(queued.unwrap_err().kind, ErrorKind::Timeout);
    for _ in 0..10 {
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            s.execute(&req("SELECT 1"), cancel).await.unwrap_err().kind,
            ErrorKind::Cancelled
        );
        assert_eq!(run(s.as_ref(), "SELECT 1").await.row_count, 1);
    }
}

#[tokio::test]
async fn special_reals_are_json_serializable() {
    let (_dir, s) = fixture().await;
    let result = run(s.as_ref(), "SELECT 1e999, -1e999").await;
    assert_eq!(
        result.table.as_ref().unwrap().rows[0],
        vec![
            Value::Float(FloatValue::Special("Infinity".into())),
            Value::Float(FloatValue::Special("-Infinity".into()))
        ]
    );
    assert!(serde_json::to_string(&result).unwrap().contains("Infinity"));
}

#[tokio::test]
async fn dropping_session_releases_uncommitted_write_lock() {
    let (dir, s) = fixture().await;
    run(s.as_ref(), "BEGIN; INSERT INTO sample(id) VALUES(42)").await;
    drop(s);
    let next = SqliteDriver::new()
        .connect(&config(&dir.path().join("test.db"), false))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match next
                .execute(
                    &req("INSERT INTO sample(id) VALUES(42)"),
                    CancelToken::new(),
                )
                .await
            {
                Ok(_) => break,
                Err(e) if e.message.contains("locked") => tokio::task::yield_now().await,
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        run(next.as_ref(), "SELECT * FROM sample WHERE id=42")
            .await
            .row_count,
        1
    );
    next.close().await;
}

#[tokio::test]
async fn change_preview_rolls_back_rows_schema_and_interruptions() {
    let dir = tempfile::tempdir().unwrap();
    let s = SqliteDriver::new()
        .connect(&config(&dir.path().join("preview.db"), false))
        .await
        .unwrap();
    run(&*s,"CREATE TABLE items(id INTEGER PRIMARY KEY,n TEXT); INSERT INTO items VALUES(1,'before'),(2,'two'); CREATE INDEX items_n ON items(n)").await;
    let original = run(&*s, "SELECT * FROM items ORDER BY id").await.table;
    for (sql, n, before, after) in [
        ("UPDATE items SET n='after' WHERE id=1", 1, true, true),
        ("DELETE FROM items WHERE id=1", 1, true, false),
        (
            "INSERT INTO items VALUES(3,'three'),(4,'four')",
            2,
            false,
            true,
        ),
    ] {
        let mut r = req(sql);
        r.preview_sample_limit = 1;
        let p = s.preview(&r, CancelToken::new()).await.unwrap();
        assert!(!p.committed);
        let st = &p.statements[0];
        assert_eq!(st.affected, Some(n));
        assert_eq!(st.before.is_some(), before);
        assert_eq!(st.after.is_some(), after);
        for table in [&st.before, &st.after].into_iter().flatten() {
            assert!(table.rows.len() <= 1);
        }
        if before {
            assert_eq!(
                st.before.as_ref().unwrap().rows[0][1],
                Value::Text("before".into())
            );
        }
        if sql.starts_with("UPDATE") {
            assert_eq!(
                st.after.as_ref().unwrap().rows[0][1],
                Value::Text("after".into())
            );
        }
        assert_eq!(
            run(&*s, "SELECT * FROM items ORDER BY id").await.table,
            original
        );
    }
    let schema = run(
        &*s,
        "SELECT type,name,sql FROM sqlite_master ORDER BY type,name",
    )
    .await
    .table;
    for sql in [
        "ALTER TABLE items ADD COLUMN extra int",
        "CREATE TABLE preview_new(x int)",
        "DROP INDEX items_n",
    ] {
        let p = s.preview(&req(sql), CancelToken::new()).await.unwrap();
        let diff = p.statements[0].schema_diff.as_ref().unwrap();
        assert!(!diff.added.is_empty() || !diff.removed.is_empty() || !diff.changed.is_empty());
        assert_eq!(
            run(
                &*s,
                "SELECT type,name,sql FROM sqlite_master ORDER BY type,name"
            )
            .await
            .table,
            schema
        );
    }
    for sql in [
        "VACUUM",
        "COMMIT",
        "PRAGMA journal_mode=WAL",
        "ATTACH ':memory:' AS danger",
        "UPDATE items SET n='bad'; COMMIT",
    ] {
        let p = s.preview(&req(sql), CancelToken::new()).await.unwrap();
        assert!(!p.notes.is_empty());
        assert!(p.statements.iter().all(|s| s.affected.is_none()));
    }
    run(&*s, "BEGIN; INSERT INTO items VALUES(3,'user')").await;
    let refused = s
        .preview(&req("DELETE FROM items"), CancelToken::new())
        .await
        .unwrap();
    assert!(refused.notes.iter().any(|n| n.contains("사용자 트랜잭션")));
    assert_eq!(run(&*s, "SELECT * FROM items").await.row_count, 3);
    run(&*s, "ROLLBACK").await;
    for timeout in [true, false] {
        let mut r=req("UPDATE items SET n='bad'; WITH RECURSIVE r(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM r WHERE x<100000000) SELECT sum(x) FROM r");
        let token = CancelToken::new();
        let other = token.clone();
        if timeout {
            r.timeout_ms = 20;
        } else {
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
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
        assert_eq!(
            run(&*s, "SELECT * FROM items ORDER BY id").await.table,
            original
        );
        run(&*s, "BEGIN; INSERT INTO items VALUES(3,'clean'); ROLLBACK").await;
    }
    // Interrupting a mutation may auto-rollback the owned savepoint; the connection must remain reusable.
    let mut mutation=req("WITH RECURSIVE r(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM r WHERE x<100000000) INSERT INTO items SELECT x+10,'bulk' FROM r");
    mutation.timeout_ms = 20;
    assert_eq!(
        s.preview(&mutation, CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    assert_eq!(
        run(&*s, "SELECT * FROM items ORDER BY id").await.table,
        original
    );
    assert!(s
        .preview(
            &req("UPDATE items SET n='bad'; SELECT missing_column"),
            CancelToken::new()
        )
        .await
        .is_err());
    assert_eq!(
        run(&*s, "SELECT * FROM items ORDER BY id").await.table,
        original
    );
}

#[tokio::test]
async fn schema_graph_and_filtered_browse() {
    let (_dir,s)=fixture().await;
    run(&*s,"CREATE TABLE parent(tenant TEXT,id INTEGER,PRIMARY KEY(tenant,id)); CREATE TABLE child(id INTEGER PRIMARY KEY,tenant TEXT,parent_id INTEGER,FOREIGN KEY(tenant,parent_id) REFERENCES parent); INSERT INTO parent VALUES('a',9223372036854775807),('b',1); INSERT INTO child VALUES(1,'a',9223372036854775807);").await;
    let graph=s.schema_graph(None).await.unwrap();let fk=graph.relations.iter().find(|r|r.from=="table:child").unwrap();assert_eq!(fk.from_fields,["tenant","parent_id"]);assert_eq!(fk.to_fields,["tenant","id"]);
    let result=s.browse(&fk.to,PageRequest {offset:0,limit:10,filter:vec![("tenant".into(),Value::Text("a".into())),("id".into(),Value::Int(i64::MAX.to_string()))]}).await.unwrap();assert_eq!(result.row_count,1);assert_eq!(result.table.unwrap().rows[0][1],Value::Int(i64::MAX.to_string()));
    let result=s.browse(&fk.to,PageRequest {offset:0,limit:10,filter:vec![("tenant".into(),Value::Text("a' OR TRUE--".into()))]}).await.unwrap();assert_eq!(result.row_count,0);
}

fn edit_request(node: &str, edits: Vec<RowEdit>) -> EditRequest {EditRequest{originals: vec![],create_table: None,node_id:node.into(),edits,timeout_ms:5000,preview_sample_limit:2}}
fn edit_key(id:i64)->Vec<(String,Value)>{vec![("id".into(),Value::Int(id.to_string()))]}
#[tokio::test]
async fn c2_bound_edits_preview_apply_conflicts_and_catalog(){

    let (_dir,s)=fixture().await;
    let table="c2_edit";let view="c2_edit_view";let no_key="c2_no_key";
    let node="table:c2_edit".to_string();let view_node="table:c2_edit_view".to_string();let no_key_node="table:c2_no_key".to_string();let bad_node="table:missing_c2; DROP TABLE x".to_string();

    run(s.as_ref(),&format!("CREATE TABLE {table}(id INTEGER PRIMARY KEY, t TEXT); INSERT INTO {table} VALUES(1,'old'),(2,'delete'); CREATE VIEW {view} AS SELECT * FROM {table}; CREATE TABLE {no_key}(t TEXT)")).await;
    let browse=s.browse(&node,PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap();
    assert_eq!(browse.edit_target.unwrap().key_columns,vec!["id"]);
    assert!(s.browse(&view_node,PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap().edit_target.is_none());
    assert!(s.browse(&no_key_node,PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap().edit_target.is_none());
    assert!(run(s.as_ref(),&format!("SELECT * FROM {table}")).await.edit_target.is_some());
    let injection="'; DROP TABLE x; -- ' \\ 한글 🦀";
    let request=edit_request(&node,vec![
        RowEdit::Update{key:edit_key(1),set:vec![("t".into(),Value::Text(injection.into()))]},
        RowEdit::Insert{values:vec![("id".into(),Value::Int("3".into())),("t".into(),Value::Text("insert".into()))]},
        RowEdit::Delete{key:edit_key(2)},
    ]);
    let preview=s.preview_edits(&request,CancelToken::new()).await.unwrap();
    assert!(!preview.committed);assert_eq!(preview.statements.len(),3);
    assert_eq!(preview.statements[0].affected,Some(1));assert!(preview.statements[0].text.contains("?"));
    assert!(!preview.statements[0].text.contains(injection));
    assert_eq!(preview.statements[0].before.as_ref().unwrap().rows[0][1],Value::Text("old".into()));
    assert_eq!(preview.statements[0].after.as_ref().unwrap().rows[0][1],Value::Text(injection.into()));
    assert_eq!(run(s.as_ref(),&format!("SELECT t FROM {table} WHERE id=1")).await.table.unwrap().rows[0][0],Value::Text("old".into()));
    let result=s.apply_edits(&request,CancelToken::new()).await.unwrap();assert_eq!(result.affected,vec![1,1,1]);
    assert_eq!(run(s.as_ref(),&format!("SELECT id,t FROM {table} ORDER BY id")).await.table.unwrap().rows,vec![vec![Value::Int("1".into()),Value::Text(injection.into())],vec![Value::Int("3".into()),Value::Text("insert".into())]]);
    let conflict=edit_request(&node,vec![RowEdit::Update{key:edit_key(1),set:vec![("t".into(),Value::Text("rollback".into()))]},RowEdit::Delete{key:edit_key(999)}]);
    for preview in [false,true]{
        let error=if preview{s.preview_edits(&conflict,CancelToken::new()).await.unwrap_err()}else{s.apply_edits(&conflict,CancelToken::new()).await.unwrap_err()};
        assert_eq!(error.kind,ErrorKind::Conflict);assert!(error.message.contains("Edit 1"));
        assert_eq!(run(s.as_ref(),&format!("SELECT t FROM {table} WHERE id=1")).await.table.unwrap().rows[0][0],Value::Text(injection.into()));
    }
    for invalid in ["missing", "t\"; DROP TABLE x; --"]{
        let bad=edit_request(&node,vec![RowEdit::Insert{values:vec![(invalid.into(),Value::Text("x".into()))]}]);
        assert_eq!(s.apply_edits(&bad,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Query);
    }
    let conversion=edit_request(&node,vec![RowEdit::Insert{values:edit_key(4)},RowEdit::Insert{values:vec![("id".into(),Value::Int("invalid integer".into()))]}]);
    let error=s.apply_edits(&conversion,CancelToken::new()).await.unwrap_err();assert_eq!(error.kind,ErrorKind::Query);assert!(error.message.contains("Edit 1"));
    assert_eq!(run(s.as_ref(),&format!("SELECT count(*) FROM {table} WHERE id=4")).await.table.unwrap().rows[0][0],Value::Int("0".into()));
    let bad=edit_request(&bad_node,request.edits.clone());assert!(s.apply_edits(&bad,CancelToken::new()).await.is_err());
    let bad=edit_request(&node,vec![RowEdit::Delete{key:vec![]}]);assert!(s.apply_edits(&bad,CancelToken::new()).await.is_err());
    let too_many=edit_request(&node,vec![RowEdit::Delete{key:edit_key(1)};1001]);assert_eq!(s.apply_edits(&too_many,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Unsupported);
    let token=CancelToken::new();token.cancel();assert_eq!(s.apply_edits(&request,token).await.unwrap_err().kind,ErrorKind::Cancelled);s.ping().await.unwrap();
    run(s.as_ref(),&format!("DROP VIEW {view}; DROP TABLE {table}; DROP TABLE {no_key}")).await;
}
#[tokio::test]
async fn c2_multirow_conflict_and_timeout_rollback(){
    let (_dir,s)=fixture().await;
    run(s.as_ref(),"CREATE TABLE c2_nullable(id INTEGER, part INTEGER, t TEXT, PRIMARY KEY(id,part)); INSERT INTO c2_nullable VALUES(1,NULL,'a'),(1,NULL,'b')").await;
    let request=edit_request("table:c2_nullable",vec![RowEdit::Insert{values:vec![("id".into(),Value::Int("5".into())),("part".into(),Value::Int("5".into()))]},RowEdit::Delete{key:vec![("id".into(),Value::Int("1".into())),("part".into(),Value::Null)]}]);
    let error=s.apply_edits(&request,CancelToken::new()).await.unwrap_err();assert_eq!(error.kind,ErrorKind::Conflict);assert!(error.message.contains("Edit 1"));
    assert_eq!(run(s.as_ref(),"SELECT count(*) FROM c2_nullable").await.table.unwrap().rows[0][0],Value::Int("2".into()));
    run(s.as_ref(),"CREATE TABLE c2_slow(id INTEGER PRIMARY KEY); CREATE TRIGGER c2_slow_trigger BEFORE INSERT ON c2_slow BEGIN SELECT sum(x) FROM (WITH RECURSIVE seq(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM seq WHERE x<100000000) SELECT x FROM seq); END").await;
    let mut request=edit_request("table:c2_slow",vec![RowEdit::Insert{values:edit_key(1)}]);
    for preview in [false,true]{
        request.timeout_ms=10;
        let error=if preview{s.preview_edits(&request,CancelToken::new()).await.unwrap_err()}else{s.apply_edits(&request,CancelToken::new()).await.unwrap_err()};assert_eq!(error.kind,ErrorKind::Timeout);s.ping().await.unwrap();
        assert_eq!(run(s.as_ref(),"SELECT count(*) FROM c2_slow").await.table.unwrap().rows[0][0],Value::Int("0".into()));
        request.timeout_ms=5000;let token=CancelToken::new();let cancelling=token.clone();
        tokio::spawn(async move{tokio::time::sleep(Duration::from_millis(10)).await;cancelling.cancel();});
        let error=if preview{s.preview_edits(&request,token).await.unwrap_err()}else{s.apply_edits(&request,token).await.unwrap_err()};assert_eq!(error.kind,ErrorKind::Cancelled);s.ping().await.unwrap();
    }
    run(s.as_ref(),"BEGIN; INSERT INTO c2_nullable VALUES(7,7,'keep')").await;
    assert_eq!(s.apply_edits(&request,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Unsupported);
    assert_eq!(s.preview_edits(&request,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Unsupported);
    run(s.as_ref(),"COMMIT").await;
}
#[tokio::test]
async fn c2_quotes_catalog_identifiers_and_avoids_temp_shadowing(){
    let (_dir,s)=fixture().await;
    run(s.as_ref(),"CREATE TABLE \"c2 quote; --\"(id INTEGER PRIMARY KEY, \"x\"\"column\" TEXT); INSERT INTO main.\"c2 quote; --\" VALUES(1,'main'); CREATE TEMP TABLE \"c2 quote; --\"(id INTEGER PRIMARY KEY, \"x\"\"column\" TEXT); INSERT INTO temp.\"c2 quote; --\" VALUES(1,'temp')").await;
    let request=edit_request("table:c2 quote; --",vec![RowEdit::Update{key:edit_key(1),set:vec![("x\"column".into(),Value::Text("edited".into()))]}]);
    assert_eq!(s.apply_edits(&request,CancelToken::new()).await.unwrap().affected,vec![1]);
    let result=s.browse("table:c2 quote; --",PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap();
    assert_eq!(result.table.unwrap().rows[0][1],Value::Text("edited".into()));
    assert_eq!(run(s.as_ref(),"SELECT \"x\"\"column\" FROM temp.\"c2 quote; --\"").await.table.unwrap().rows[0][0],Value::Text("temp".into()));
}

#[tokio::test]
async fn c2_binary_edit_keeps_blob_storage_and_exact_bytes() {
    let (_dir, s) = fixture().await;
    run(s.as_ref(), "CREATE TABLE c2_binary(id INTEGER PRIMARY KEY, payload BLOB); INSERT INTO c2_binary VALUES(1, X'00')").await;
    let request = edit_request("table:c2_binary", vec![RowEdit::Update {
        key: edit_key(1),
        set: vec![("payload".into(), Value::Bytes { len: 4, hex: "00a1b2ff".into() })],
    }]);
    assert_eq!(s.apply_edits(&request, CancelToken::new()).await.unwrap().affected, vec![1]);
    let result = run(s.as_ref(), "SELECT payload, typeof(payload) FROM c2_binary WHERE id=1").await;
    let table = result.table.unwrap();
    assert_eq!(table.rows[0][0], Value::Bytes { len: 4, hex: "00a1b2ff".into() });
    assert_eq!(table.rows[0][1], Value::Text("blob".into()));
}

#[tokio::test]
async fn query_result_edit_provenance_and_conflict() {
    let (_dir, s) = fixture().await;
    let result = run(&*s, "SELECT id, t, id+1 AS calculated FROM sample WHERE id > 0").await;
    assert!(result.edit_target.is_some(), "single-table query with its primary key must be editable");
    for sql in ["SELECT t FROM sample", "SELECT DISTINCT id,t FROM sample", "SELECT id,count(*) FROM sample GROUP BY id", "SELECT a.id,a.t FROM sample a JOIN sample b ON a.id=b.id", "SELECT * FROM sample_view"] {
        assert!(run(&*s, sql).await.edit_target.is_none(), "must be read-only: {sql}");
    }
    run(&*s,"CREATE TABLE d10_many(id INTEGER PRIMARY KEY,name TEXT); WITH RECURSIVE r(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM r WHERE x<1001) INSERT INTO d10_many SELECT x,'name' FROM r").await;
    let mut many=req("SELECT * FROM d10_many");many.row_limit=2000;
    let bounded=s.execute(&many,CancelToken::new()).await.unwrap().remove(0);
    assert_eq!(bounded.row_count,1001);assert!(!bounded.truncated);assert!(bounded.edit_target.is_none());assert!(bounded.edit_disabled_reason.unwrap().contains("1,000"));
    let request: EditRequest = serde_json::from_value(serde_json::json!({"node_id":"table:sample","edits":[{"op":"Update","key":[["id",{"t":"Int","v":i64::MAX.to_string()}]],"set":[["t",{"t":"Text","v":"edited"}]]}],"originals":[[["t",{"t":"Null"}]]]})).unwrap();
    s.preview_edits(&request, CancelToken::new()).await.unwrap();
    assert_eq!(run(&*s, "SELECT t FROM sample").await.table.unwrap().rows[0][0], Value::Null);
    run(&*s, "UPDATE sample SET t='concurrent'").await;
    assert!(matches!(s.apply_edits(&request, CancelToken::new()).await, Err(DbError{kind: ErrorKind::Conflict,..})));
    assert_eq!(run(&*s, "SELECT t FROM sample").await.table.unwrap().rows[0][0], Value::Text("concurrent".into()));
    let mut conflicting=request.clone();
    conflicting.originals=vec![vec![("t".into(),Value::Text("concurrent".into()))],vec![]];
    conflicting.edits.push(RowEdit::Update{key:vec![("id".into(),Value::Int("999".into()))],set:vec![("t".into(),Value::Text("missing".into()))]});
    assert!(matches!(s.apply_edits(&conflicting,CancelToken::new()).await,Err(DbError{kind:ErrorKind::Conflict,..})));
    assert_eq!(run(&*s,"SELECT t FROM sample").await.table.unwrap().rows[0][0],Value::Text("concurrent".into()));

}
