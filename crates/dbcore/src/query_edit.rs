//! Conservative query shape checks; only drivers determine column provenance.
use crate::*;
use sqlparser::{
    ast::{Expr, GroupByExpr, SetExpr, Statement, TableFactor, Visit, Visitor},
    dialect::Dialect,
    parser::Parser,
};
use std::ops::ControlFlow;

pub fn source(sql: &str, dialect: &dyn Dialect) -> Result<Vec<String>, &'static str> {
    let statements =
        Parser::parse_sql(dialect, sql).map_err(|_| "쿼리 출처를 확인할 수 없습니다")?;
    let [Statement::Query(query)] = statements.as_slice() else {
        return Err("단일 SELECT 결과만 편집할 수 있습니다");
    };
    let SetExpr::Select(select) = &*query.body else {
        return Err("집합 결과는 편집할 수 없습니다");
    };
    if query.with.is_some() || select.from.len() != 1 || !select.from[0].joins.is_empty() {
        return Err("여러 테이블 결과는 편집할 수 없습니다");
    }
    if select.distinct.is_some()
        || !matches!(&select.group_by, GroupByExpr::Expressions(e,m) if e.is_empty() && m.is_empty())
        || select.having.is_some()
        || select.into.is_some()
        || !select.named_window.is_empty()
        || select.qualify.is_some()
    {
        return Err("집계·DISTINCT·GROUP BY 결과는 편집할 수 없습니다");
    }
    struct Shape;
    impl Visitor for Shape {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
            // shortcut: functions may be user-defined aggregates; allow scalar functions once catalog classification is available.
            if matches!(
                expr,
                Expr::Function(_) | Expr::Subquery(_) | Expr::Exists { .. }
            ) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
    }
    if select.visit(&mut Shape).is_break() {
        return Err("집계 또는 함수·하위 쿼리 결과는 편집할 수 없습니다");
    }
    // shortcut: only one direct table is supported; add joins/CTEs when row identity can be proven.
    let TableFactor::Table {
        name,
        args: None,
        version: None,
        ..
    } = &select.from[0].relation
    else {
        return Err("단일 테이블 결과만 편집할 수 있습니다");
    };
    name.0
        .iter()
        .map(|part| {
            part.as_ident()
                .map(|i| {
                    if dialect.is::<sqlparser::dialect::PostgreSqlDialect>()
                        && i.quote_style.is_none()
                    {
                        i.value.to_lowercase()
                    } else {
                        i.value.clone()
                    }
                })
                .ok_or("테이블 출처를 확인할 수 없습니다")
        })
        .collect()
}

pub fn unique_labels<'a>(mut labels: impl Iterator<Item = &'a str>) -> Result<(), &'static str> {
    let mut seen = std::collections::HashSet::new();
    if labels.any(|name| !seen.insert(name)) {
        Err("중복 결과 컬럼 이름은 편집할 수 없습니다")
    } else {
        Ok(())
    }
}

pub fn target(
    node_id: String,
    keys: Vec<String>,
    columns: Vec<Option<String>>,
) -> Result<EditTarget, &'static str> {
    if keys.is_empty() {
        return Err("기본 키가 없는 테이블입니다");
    }
    if keys
        .iter()
        .any(|key| !columns.iter().any(|name| name.as_ref() == Some(key)))
    {
        return Err("기본 키가 결과에 없습니다");
    }
    let mut seen = std::collections::HashSet::new();
    if columns.iter().flatten().any(|name| !seen.insert(name)) {
        return Err("중복 원본 컬럼은 편집할 수 없습니다");
    }
    if columns.iter().flatten().all(|name| keys.contains(name)) {
        return Err("계산 컬럼: 수정 가능한 원본 컬럼이 없습니다");
    }
    Ok(EditTarget {
        node_id,
        key_columns: keys,
        columns,
    })
}
pub fn finish(
    result: &mut ResultSet,
    candidate: Result<EditTarget, &'static str>,
    read_only: bool,
) {
    // shortcut: EditableTable mounts all rows; raise this cap only after editor virtualization.
    let candidate = if read_only {
        Err("읽기 전용 연결 또는 실행입니다")
    } else if result.row_count > 1000 {
        Err("쿼리 결과 편집은 1,000행까지 지원합니다. 조회 범위를 줄이세요")
    } else if result.truncated {
        Err("행 상한을 초과한 결과는 편집할 수 없습니다")
    } else {
        candidate
    };
    match candidate {
        Ok(target) => {
            result.edit_target = Some(target);
            result.edit_disabled_reason = None;
        }
        Err(reason) => {
            result.edit_target = None;
            result.edit_disabled_reason = Some(reason.into());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::GenericDialect;
    #[test]
    fn only_direct_single_table_selects() {
        assert_eq!(
            source(
                "SELECT id, n+1 AS computed FROM items WHERE id=1",
                &GenericDialect
            )
            .unwrap(),
            ["items"]
        );
        for sql in [
            "SELECT DISTINCT id FROM items",
            "SELECT id,count(*) FROM items GROUP BY id",
            "SELECT a.id FROM items a JOIN items b ON a.id=b.id",
            "SELECT id FROM items UNION SELECT id FROM items",
            "SELECT id,sum(n) OVER () FROM items",
            "WITH x AS (SELECT * FROM items) SELECT * FROM x",
        ] {
            assert!(source(sql, &GenericDialect).is_err(), "{sql}");
        }
    }
    #[test]
    fn projection_requires_all_keys_and_nonkey_origin() {
        assert!(
            target(
                "t".into(),
                vec!["id".into()],
                vec![None, Some("name".into())]
            )
            .is_err()
        );
        assert!(
            target(
                "t".into(),
                vec!["id".into(), "part".into()],
                vec![Some("id".into()), Some("name".into())]
            )
            .is_err()
        );
        let t = target(
            "t".into(),
            vec!["id".into()],
            vec![Some("id".into()), Some("name".into()), None],
        )
        .unwrap();
        assert_eq!(t.columns[2], None);
    }
}
