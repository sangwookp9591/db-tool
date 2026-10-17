//! Conservative SQL sampling plans. Unknown shapes execute unchanged, without guessed samples.
use crate::{DbError, SchemaDiff, StatementKind};
use std::collections::BTreeMap;

pub fn schema_diff(
    before: BTreeMap<String, String>,
    after: BTreeMap<String, String>,
) -> SchemaDiff {
    SchemaDiff {
        added: after
            .keys()
            .filter(|k| !before.contains_key(*k))
            .cloned()
            .collect(),
        removed: before
            .keys()
            .filter(|k| !after.contains_key(*k))
            .cloned()
            .collect(),
        changed: after
            .iter()
            .filter(|(k, v)| before.get(*k).is_some_and(|old| old != *v))
            .map(|(k, _)| k.clone())
            .collect(),
    }
}
#[derive(Debug)]
pub struct Token<'a> {
    pub text: &'a str,
    pub start: usize,
    pub depth: usize,
}
impl Token<'_> {
    pub fn is(&self, word: &str) -> bool {
        self.text.eq_ignore_ascii_case(word)
    }
}
/// Preserve offsets while excluding comments and treating quoted strings/identifiers atomically.
pub fn tokens(sql: &str) -> Result<Vec<Token<'_>>, DbError> {
    let b = sql.as_bytes();
    let (mut i, mut depth) = (0, 0usize);
    let mut out = Vec::new();
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"--") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            i += 2;
            let mut comments = 1;
            while i < b.len() && comments > 0 {
                if b[i..].starts_with(b"/*") {
                    comments += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    comments -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if comments != 0 {
                return Err(DbError::query("미리보기 SQL의 주석이 닫히지 않았습니다"));
            }
            continue;
        }
        let start = i;
        if matches!(b[i], b'\'' | b'"' | b'`' | b'[') {
            let quote = if b[i] == b'[' { b']' } else { b[i] };
            let escape = quote == b'\'' && i > 0 && matches!(b[i - 1], b'e' | b'E');
            i += 1;
            let mut closed = false;
            while i < b.len() {
                if escape && b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == quote {
                    i += 1;
                    if b.get(i) == Some(&quote) {
                        i += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            if !closed {
                return Err(DbError::query("미리보기 SQL의 따옴표가 닫히지 않았습니다"));
            }
        } else if b[i] == b'$' {
            let mut end = i + 1;
            while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                end += 1;
            }
            if b.get(end) == Some(&b'$') {
                let tag = &sql[i..=end];
                let close = sql[end + 1..]
                    .find(tag)
                    .ok_or_else(|| DbError::query("미리보기 SQL의 달러 인용(dollar quote)이 닫히지 않았습니다"))?;
                i = end + 1 + close + tag.len();
            } else {
                i += 1;
            }
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' || b[i] >= 128 {
            i += 1;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] >= 128)
            {
                i += 1;
            }
        } else {
            i += 1;
        }
        if &sql[start..i] == ")" {
            depth = depth.saturating_sub(1);
        }
        out.push(Token {
            text: &sql[start..i],
            start,
            depth,
        });
        if &sql[start..i] == "(" {
            depth += 1;
        }
    }
    Ok(out)
}
pub fn kind(tokens: &[Token<'_>]) -> StatementKind {
    match tokens
        .first()
        .map(|t| t.text.to_ascii_uppercase())
        .as_deref()
    {
        Some("INSERT" | "REPLACE") => StatementKind::Insert,
        Some("UPDATE") => StatementKind::Update,
        Some("DELETE") => StatementKind::Delete,
        Some("CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "REINDEX") => StatementKind::Ddl,
        _ => StatementKind::Other,
    }
}
pub fn rejection(tokens: &[Token<'_>]) -> Option<String> {
    let first = tokens.first()?;
    if !matches!(kind(tokens), StatementKind::Other)
        || first.is("SELECT")
        || first.is("WITH")
        || first.is("VALUES")
    {
        if tokens.iter().any(|t| {
            [
                "CONCURRENTLY",
                "DATABASE",
                "TABLESPACE",
                "SUBSCRIPTION",
                "VIRTUAL",
            ]
            .iter()
            .any(|w| t.is(w))
        }) {
            return Some("미리보기를 거부했습니다: 동시 실행, 데이터베이스, 테이블스페이스, 구독, 가상 테이블 작업은 안전하게 롤백할 수 없습니다.".into());
        }
        None
    } else {
        Some(format!(
            "미리보기를 거부했습니다: {}은(는) 트랜잭션·세션 제어이거나 롤백을 지원하지 않습니다.",
            first.text
        ))
    }
}
// shortcut: single-table WHERE sampling only; use a SQL AST when joins/CTE before images are required.
pub fn before_query(sql: &str, ts: &[Token<'_>], limit: u32, lock: bool) -> Option<String> {
    let is_update = ts.first()?.is("UPDATE");
    if !is_update && !ts.first()?.is("DELETE") {
        return None;
    }
    let start = if is_update {
        1
    } else {
        if !ts.get(1)?.is("FROM") {
            return None;
        }
        2
    };
    let identifier = |t: &Token<'_>| {
        t.text.starts_with('"')
            || t.text.starts_with('`')
            || t.text.starts_with('[')
            || t.text
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b >= 128)
    };
    if !identifier(ts.get(start)?) {
        return None;
    }
    let mut end = start + 1;
    while ts.get(end).is_some_and(|t| t.text == ".") {
        if !identifier(ts.get(end + 1)?) {
            return None;
        }
        end += 2;
    }
    let table_end = ts[end - 1].start + ts[end - 1].text.len();
    let tail = ts.get(end);
    if is_update && !tail.is_some_and(|t| t.is("SET")) {
        return None;
    }
    if !is_update && tail.is_some_and(|t| !t.is("WHERE") && !t.is("RETURNING")) {
        return None;
    }
    if ts[end..]
        .iter()
        .any(|t| t.depth == 0 && ["FROM", "USING", "ORDER", "LIMIT"].iter().any(|w| t.is(w)))
    {
        return None;
    }
    let where_token = ts.iter().find(|t| t.depth == 0 && t.is("WHERE"));
    let returning = ts.iter().find(|t| t.depth == 0 && t.is("RETURNING"));
    let predicate = where_token
        .map(|t| &sql[t.start..returning.map_or(sql.len(), |r| r.start)])
        .unwrap_or("");
    Some(format!(
        "SELECT * FROM {}\n{}\nLIMIT {}{}",
        &sql[ts[start].start..table_end],
        predicate,
        limit,
        if lock { " FOR UPDATE" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_respect_comments_quotes_nested_keywords_and_unsafe_controls() {
        let sql = "/* SET; */ UPDATE \"odd.table\" SET name='WHERE RETURNING' WHERE id IN (SELECT id FROM x) -- tail";
        let ts = tokens(sql).unwrap();
        assert_eq!(kind(&ts), StatementKind::Update);
        let before = before_query(sql, &ts, 2, true).unwrap();
        assert!(before.contains("FROM \"odd.table\"\nWHERE id IN"));
        assert!(before.ends_with("\nLIMIT 2 FOR UPDATE"));
        for sql in [
            "COMMIT",
            "VACUUM",
            "CREATE INDEX CONCURRENTLY idx ON t(x)",
            "CREATE DATABASE d",
            "ATTACH 'x' AS y",
            "PRAGMA writable_schema=1",
        ] {
            assert!(rejection(&tokens(sql).unwrap()).is_some(), "{sql}");
        }
        for sql in [
            "UPDATE t SET x=1 FROM q WHERE t.id=q.id",
            "DELETE FROM t AS alias WHERE id=1",
            "WITH x AS (SELECT 1) UPDATE t SET x=1",
        ] {
            assert!(before_query(sql, &tokens(sql).unwrap(), 50, true).is_none());
        }
    }
}
