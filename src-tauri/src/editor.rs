use crate::{CommandResult, Host};
use dbcore::{CancelToken, DbError, ErrorKind, PlanTree, SchemaGraph};
use serde::{Deserialize, Serialize};
use sqlparser::{
    ast::{
        Expr, Ident, JoinOperator, Query, Select, SelectItem, SelectItemQualifiedWildcardKind,
        SetExpr, Spanned, Statement, TableFactor, Visit, Visitor,
    },
    dialect::{Dialect, GenericDialect, PostgreSqlDialect, SQLiteDialect},
    parser::Parser,
    tokenizer::{Location, Span, Token, Tokenizer, Whitespace},
};
use std::ops::ControlFlow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<TextEdit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: String,
    pub start: usize,
    pub end: usize,
    pub severity: String,
    pub message: String,
    pub quick_fix: Option<QuickFix>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefactorAction {
    Format,
    ExpandStar,
    RenameAlias { from: String, to: String },
    ExtractCte { name: String },
}
fn dialect(language: &str) -> Box<dyn Dialect> {
    match language {
        "postgresql" | "postgres" => Box::new(PostgreSqlDialect {}),
        "sqlite" => Box::new(SQLiteDialect {}),
        _ => Box::new(GenericDialect {}),
    }
}
fn parse(text: &str, language: &str) -> Result<Vec<Statement>, DbError> {
    if text.len() > 256 * 1024 {
        return Err(DbError::query(
            "쿼리가 편집기 분석 한도(256 KiB)를 초과했습니다.",
        ));
    }
    Parser::parse_sql(dialect(language).as_ref(), text).map_err(|e| DbError::query(e.to_string()))
}
fn offset(text: &str, loc: Location) -> usize {
    let mut line = 1;
    let mut column = 1;
    for (i, c) in text.char_indices() {
        if line == loc.line && column == loc.column {
            return i;
        }
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    text.len()
}
fn edit(text: &str, start: usize, end: usize, replacement: String) -> TextEdit {
    TextEdit {
        start: text[..start].encode_utf16().count(),
        end: text[..end].encode_utf16().count(),
        text: replacement,
    }
}
fn span_edit(text: &str, span: Span, replacement: String) -> TextEdit {
    edit(
        text,
        offset(text, span.start),
        offset(text, span.end),
        replacement,
    )
}
fn diagnostic(text: &str, span: Span, rule: &str, severity: &str, message: &str) -> Diagnostic {
    let e = span_edit(text, span, String::new());
    Diagnostic {
        rule_id: rule.into(),
        start: e.start,
        end: e.end.max(e.start + 1).min(text.encode_utf16().count()),
        severity: severity.into(),
        message: message.into(),
        quick_fix: None,
    }
}
struct Linter<'a> {
    text: &'a str,
    diagnostics: Vec<Diagnostic>,
}
impl Visitor for Linter<'_> {
    type Break = ();
    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        let missing = matches!(statement, Statement::Update(u) if u.selection.is_none())
            || matches!(statement, Statement::Delete(d) if d.selection.is_none());
        if missing {
            self.diagnostics.push(diagnostic(
                self.text,
                statement.span(),
                "missing_where",
                "error",
                "WHERE 없는 UPDATE/DELETE는 일치하는 모든 행에 적용됩니다. 검토한 조건을 추가하세요.",
            ));
        }
        ControlFlow::Continue(())
    }
    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<()> {
        for item in &select.projection {
            if matches!(
                item,
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(..)
            ) {
                let d = diagnostic(
                    self.text,
                    item.span(),
                    "select_star",
                    "warning",
                    "필요한 컬럼만 명시해 불필요한 데이터 전송과 스키마 변경 영향을 줄이세요.",
                );
                self.diagnostics.push(d);
            }
        }
        if select.from.len() > 1
            || select.from.iter().any(|f| {
                f.joins
                    .iter()
                    .any(|j| matches!(j.join_operator, JoinOperator::CrossJoin(_)))
            })
        {
            self.diagnostics.push(diagnostic(
                self.text,
                select.span(),
                "cartesian_product",
                "warning",
                "카테시안 곱으로 결과 행이 늘어날 수 있습니다. JOIN 조건을 검토하세요.",
            ));
        }
        ControlFlow::Continue(())
    }
}
fn cypher_code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut code = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if matches!(bytes[i], b'\'' | b'"' | b'`') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == quote {
                    i += 1;
                    if bytes.get(i) != Some(&quote) {
                        break;
                    }
                    i += 1;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i..].starts_with(b"--") {
            // A Cypher edge must not become an SQL line comment in the shared tokenizer.
            i += 2;
        } else {
            i += 1;
            continue;
        }
        code[start..i].fill(b' ');
    }
    // Mask whole UTF-8 literals/comments with ASCII spaces, keeping original byte offsets.
    String::from_utf8(code).unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}
pub fn lint(text: &str, language: &str) -> Result<Vec<Diagnostic>, DbError> {
    if text.len() > 256 * 1024 {
        return Err(DbError::query(
            "쿼리가 편집기 분석 한도(256 KiB)를 초과했습니다.",
        ));
    }
    if language == "cypher" {
        let brackets = cypher_code(text).replace('[', "(").replace(']', ")");
        let ts = dbcore::preview::tokens(&brackets)?;
        let mut diagnostics = vec![];
        let mut bracket = None;
        for (i, t) in ts.iter().enumerate() {
            if text.as_bytes().get(t.start) == Some(&b'[') {
                bracket = Some(i);
            }
            if text.as_bytes().get(t.start) == Some(&b']') {
                bracket = None;
            }
            if t.text == "*" && bracket.is_some() {
                let tail = &ts[i + 1..];
                let end = tail
                    .iter()
                    .position(|t| text.as_bytes().get(t.start) == Some(&b']'))
                    .unwrap_or(0);
                let bound = &tail[..end];
                let dots = bound.iter().position(|t| t.text == ".");
                if bound.is_empty()
                    || dots.is_some_and(|j| {
                        !bound[j + 1..]
                            .iter()
                            .any(|t| t.text.chars().all(|c| c.is_ascii_digit()))
                    })
                {
                    let mut d = Diagnostic {
                        rule_id: "cypher_unbounded_path".into(),
                        start: text[..t.start].encode_utf16().count(),
                        end: text[..t.start + 1].encode_utf16().count(),
                        severity: "warning".into(),
                        message: "가변 길이 관계의 상한이 지정되지 않았습니다.".into(),
                        quick_fix: None,
                    };
                    if bound.is_empty() {
                        d.quick_fix = Some(QuickFix {
                            title: "최대 5단계로 제한".into(),
                            edits: vec![edit(text, t.start, t.start + 1, "*1..5".into())],
                        });
                    }
                    diagnostics.push(d);
                }
            }
        }
        return Ok(diagnostics);
    }
    if language != "sql"
        && language != "postgresql"
        && language != "postgres"
        && language != "sqlite"
    {
        return Ok(vec![]);
    }
    match parse(text, language) {
        Ok(statements) => {
            let mut l = Linter {
                text,
                diagnostics: vec![],
            };
            let _ = statements.visit(&mut l);
            Ok(l.diagnostics)
        }
        Err(e) => Ok(vec![Diagnostic {
            rule_id: "syntax".into(),
            start: 0,
            end: text.encode_utf16().count(),
            severity: "error".into(),
            message: e.message,
            quick_fix: None,
        }]),
    }
}
fn single_query(statements: &[Statement]) -> Result<(&Query, &Select), DbError> {
    if let [Statement::Query(q)] = statements
        && let SetExpr::Select(s) = q.body.as_ref()
    {
        return Ok((q, s));
    }
    Err(DbError::unsupported("리팩터링하려면 SELECT 쿼리 하나가 필요합니다."))
}
fn simple_table(select: &Select) -> Result<&TableFactor, DbError> {
    match select.from.as_slice() {
        [table] if table.joins.is_empty() => Ok(&table.relation),
        _ => Err(DbError::unsupported(
            "리팩터링하려면 원본 테이블 하나만 사용해야 합니다.",
        )),
    }
}
fn same_identifier(a: &Ident, b: &Ident) -> bool {
    let resolved = |i: &Ident| {
        if i.quote_style.is_some() {
            i.value.clone()
        } else {
            i.value.to_ascii_lowercase()
        }
    };
    resolved(a) == resolved(b)
}
fn identifier(name: &str) -> Result<String, DbError> {
    if name.is_empty() || name.chars().count() > 63 || name.chars().any(|c| c == '\0') {
        return Err(DbError::query("식별자가 올바르지 않습니다."));
    }
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}
pub fn refactor(
    text: &str,
    language: &str,
    action: RefactorAction,
    schema: &SchemaGraph,
) -> Result<Vec<TextEdit>, DbError> {
    let statements = parse(text, language)?;
    let mut edits = vec![];
    match action {
        RefactorAction::Format => {
            let tokens = Tokenizer::new(dialect(language).as_ref(), text)
                .tokenize()
                .map_err(|e| DbError::query(e.to_string()))?;
            if tokens.iter().any(|t| {
                matches!(
                    t,
                    Token::Whitespace(
                        Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_)
                    )
                )
            }) {
                return Err(DbError::unsupported(
                    "주석이 포함된 쿼리는 서식 정리를 지원하지 않습니다. 주석은 유지됩니다.",
                ));
            }
            let formatted = statements
                .iter()
                .map(|s| format!("{s:#}"))
                .collect::<Vec<_>>()
                .join(";\n");
            edits.push(edit(
                text,
                0,
                text.len(),
                formatted
                    + if text.trim_end().ends_with(';') {
                        ";"
                    } else {
                        ""
                    },
            ));
        }
        RefactorAction::ExpandStar => {
            let (q, select) = single_query(&statements)?;
            if q.with.is_some() {
                return Err(DbError::unsupported(
                    "CTE의 별표를 펼치려면 스키마 정보가 필요합니다.",
                ));
            }
            let TableFactor::Table {
                name,
                alias,
                args: None,
                ..
            } = simple_table(select)?
            else {
                return Err(DbError::unsupported("별표를 펼치려면 스키마가 있는 테이블이 필요합니다."));
            };
            if alias.as_ref().is_some_and(|a| !a.columns.is_empty()) {
                return Err(DbError::unsupported(
                    "컬럼 별칭을 처리하려면 스키마 정보가 필요합니다.",
                ));
            }
            let table_name = name.to_string();
            let leaf = name
                .0
                .last()
                .and_then(|p| p.as_ident())
                .map(|i| i.value.as_str())
                .ok_or_else(|| DbError::query("테이블 식별자가 올바르지 않습니다."))?;
            let matches: Vec<_> = schema
                .entities
                .iter()
                .filter(|e| {
                    e.name == table_name
                        || e.name == leaf
                        || e.name.rsplit('.').next() == Some(leaf)
                })
                .collect();
            let [entity] = matches.as_slice() else {
                return Err(DbError::query("테이블 스키마가 없거나 여러 테이블과 일치합니다."));
            };
            if entity.fields.is_empty() {
                return Err(DbError::query("테이블의 컬럼 정보를 확인할 수 없습니다."));
            }
            let qualifier = alias
                .as_ref()
                .map(|a| a.name.to_string())
                .unwrap_or(table_name);
            let replacement = entity
                .fields
                .iter()
                .map(|f| identifier(&f.name).map(|name| format!("{qualifier}.{name}")))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            for item in &select.projection {
                if matches!(
                    item,
                    SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(..)
                ) {
                    // shortcut: wildcard modifiers and expression wildcards need resolved output schemas; reject until supported.
                    let raw = item.to_string();
                    if raw != "*"
                        && raw != format!("{qualifier}.*")
                        && !(alias.is_none() && raw == format!("{leaf}.*"))
                    {
                        return Err(DbError::unsupported(
                            "와일드카드 한정자 또는 알 수 없는 한정자는 지원하지 않습니다.",
                        ));
                    }
                    edits.push(span_edit(text, item.span(), replacement.clone()));
                }
            }
        }
        RefactorAction::RenameAlias { from, to } => {
            let (q, select) = single_query(&statements)?;
            let TableFactor::Table {
                alias: Some(alias), ..
            } = simple_table(select)?
            else {
                return Err(DbError::unsupported(
                    "이름을 바꾸려면 이름이 지정된 테이블 별칭 하나가 필요합니다.",
                ));
            };
            if alias.name.value != from {
                return Err(DbError::query("별칭을 찾을 수 없습니다."));
            }
            struct ScopeCount {
                queries: usize,
            }
            impl Visitor for ScopeCount {
                type Break = ();
                fn pre_visit_query(&mut self, _: &Query) -> ControlFlow<()> {
                    self.queries += 1;
                    ControlFlow::Continue(())
                }
            }
            let mut scopes = ScopeCount { queries: 0 };
            let _ = q.visit(&mut scopes);
            if scopes.queries != 1 || q.with.is_some() {
                return Err(DbError::unsupported(
                    "중첩 별칭 범위를 처리하려면 의미 분석이 필요합니다.",
                ));
            }
            let replacement = identifier(&to)?;
            struct References<'a> {
                text: &'a str,
                name: &'a Ident,
                replacement: &'a str,
                edits: Vec<TextEdit>,
                ambiguous: bool,
            }
            impl Visitor for References<'_> {
                type Break = ();
                fn pre_visit_expr(&mut self, e: &Expr) -> ControlFlow<()> {
                    if let Expr::Identifier(id) = e
                        && same_identifier(id, self.name)
                    {
                        self.ambiguous = true;
                    }
                    if let Expr::CompoundIdentifier(ids) = e
                        && ids.first().is_some_and(|i| same_identifier(i, self.name))
                    {
                        self.edits.push(span_edit(
                            self.text,
                            ids[0].span,
                            self.replacement.into(),
                        ));
                    }
                    ControlFlow::Continue(())
                }
            }
            let mut refs = References {
                text,
                name: &alias.name,
                replacement: &replacement,
                edits: vec![],
                ambiguous: false,
            };
            let _ = q.visit(&mut refs);
            if refs.ambiguous {
                return Err(DbError::unsupported(
                    "한정되지 않은 전체 행 별칭 참조를 처리하려면 의미 분석이 필요합니다.",
                ));
            }
            edits = refs.edits;
            edits.push(span_edit(text, alias.name.span, replacement.clone()));
            for item in &select.projection {
                if let SelectItem::QualifiedWildcard(
                    SelectItemQualifiedWildcardKind::ObjectName(name),
                    _,
                ) = item
                    && let [part] = name.0.as_slice()
                    && let Some(id) = part.as_ident().filter(|i| same_identifier(i, &alias.name))
                {
                    edits.push(span_edit(text, id.span, replacement.clone()));
                }
            }
        }
        RefactorAction::ExtractCte { name } => {
            let (q, select) = single_query(&statements)?;
            if q.with.is_some() {
                return Err(DbError::unsupported(
                    "기존 WITH 절에 추출하려면 이름 확인이 필요합니다.",
                ));
            }
            let TableFactor::Derived { lateral: false, .. } = simple_table(select)? else {
                return Err(DbError::unsupported(
                    "추출하려면 FROM 절에 비-lateral 하위 쿼리 하나가 필요합니다.",
                ));
            };
            let tokens = dbcore::preview::tokens(text)?;
            if tokens.iter().any(|t| {
                t.text.eq_ignore_ascii_case(&name)
                    || t.text == identifier(&name).unwrap_or_default()
            }) {
                return Err(DbError::query(
                    "CTE 이름이 기존 식별자와 겹칩니다.",
                ));
            }
            let name = identifier(&name)?;
            let tokens = dbcore::preview::tokens(text)?;
            let from = tokens
                .iter()
                .position(|t| t.depth == 0 && t.is("FROM"))
                .ok_or_else(|| DbError::query("FROM 절을 찾을 수 없습니다."))?;
            let open = tokens
                .get(from + 1)
                .filter(|t| t.text == "(")
                .ok_or_else(|| DbError::query("하위 쿼리를 찾을 수 없습니다."))?;
            let close = tokens
                .iter()
                .skip(from + 2)
                .find(|t| t.text == ")" && t.depth == 0)
                .ok_or_else(|| DbError::query("닫는 괄호를 찾을 수 없습니다."))?;
            let inner = &text[open.start + 1..close.start];
            edits.push(edit(text, 0, 0, format!("WITH {name} AS ({inner})\n")));
            edits.push(edit(text, open.start, close.start + 1, name));
        }
    }
    edits.sort_by_key(|e| e.start);
    Ok(edits)
}

async fn projection_schema(
    session: &dyn dbcore::Session,
    query: &str,
    language: &str,
) -> Result<SchemaGraph, DbError> {
    let statements = parse(query, language)?;
    let (q, select) = single_query(&statements)?;
    if q.with.is_some() {
        return Err(DbError::unsupported(
            "CTE의 컬럼 스키마를 확인할 수 없습니다.",
        ));
    }
    let TableFactor::Table {
        name, args: None, ..
    } = simple_table(select)?
    else {
        return Err(DbError::unsupported("컬럼 스키마를 확인하려면 테이블이 필요합니다."));
    };
    // Zero-row metadata resolves search_path, quoted names and temporary tables without sampling data.
    let req = dbcore::ExecRequest {
        text: format!("SELECT * FROM {name} LIMIT 0"),
        read_only: true,
        params: Default::default(),
        row_limit: 1,
        graph_node_limit: 1,
        timeout_ms: 5000,
        preview_sample_limit: 0,
    };
    let results = session.execute(&req, CancelToken::new()).await?;
    let table = results
        .into_iter()
        .next()
        .and_then(|r| r.table)
        .ok_or_else(|| DbError::query("컬럼 메타데이터가 반환되지 않았습니다."))?;
    Ok(SchemaGraph {
        entities: vec![dbcore::SchemaEntity {
            id: name.to_string(),
            name: name.to_string(),
            kind: "table".into(),
            fields: table
                .columns
                .into_iter()
                .map(|c| dbcore::SchemaField {
                    name: c.name,
                    type_name: c.type_name,
                    pk: false,
                    nullable: true,
                })
                .collect(),
        }],
        relations: vec![],
    })
}

#[tauri::command]
pub async fn explain_query(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    query: String,
    analyze: bool,
) -> CommandResult<PlanTree> {
    if query.len() > 256 * 1024 {
        return Err(DbError::query("쿼리가 편집기 분석 한도(256 KiB)를 초과했습니다."));
    }
    let record = host.session(&session_id)?;
    if !host
        .driver(&record.profile.driver_id)?
        .manifest()
        .capabilities
        .explain
    {
        return Err(DbError::unsupported("이 드라이버는 실행 계획 분석을 지원하지 않습니다."));
    }
    if analyze && host.access_mode(&record)? == crate::permissions::AccessMode::ReadOnly {
        return Err(DbError::new(
            ErrorKind::Auth,
            "읽기 전용 연결에서는 ANALYZE를 사용할 수 없습니다.",
        ));
    }
    let run = host.run(&session_id, &run_id)?;
    host.observe(
        &record,
        record
            .session
            .explain(&query, analyze, run.token.clone())
            .await,
    )
}
#[tauri::command]
pub async fn lint_query(
    host: tauri::State<'_, Host>,
    session_id: String,
    query: String,
) -> CommandResult<Vec<Diagnostic>> {
    let record = host.session(&session_id)?;
    let driver = host.driver(&record.profile.driver_id)?;
    if !driver.manifest().capabilities.lint {
        return Err(DbError::unsupported("이 드라이버 언어에는 린트 규칙이 없습니다."));
    }
    // Automatic editor lint runs after each pause in typing. Keep it local: metadata
    // and planner calls queue behind foreground work and cannot be cancelled by
    // this command's API. Explicit refactoring/explain own those database reads.
    lint(&query, &driver.manifest().query_language.id)
}
#[tauri::command]
pub async fn refactor_query(
    host: tauri::State<'_, Host>,
    session_id: String,
    query: String,
    action: RefactorAction,
) -> CommandResult<Vec<TextEdit>> {
    let record = host.session(&session_id)?;
    let driver = host.driver(&record.profile.driver_id)?;
    if driver.manifest().query_language.editor_mode != "sql" {
        return Err(DbError::unsupported("SQL 리팩터링만 지원합니다."));
    }
    let schema = if matches!(action, RefactorAction::ExpandStar) {
        projection_schema(
            record.session.as_ref(),
            &query,
            &driver.manifest().query_language.id,
        )
        .await?
    } else {
        SchemaGraph {
            entities: vec![],
            relations: vec![],
        }
    };
    refactor(
        &query,
        &driver.manifest().query_language.id,
        action,
        &schema,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn automatic_lint_never_reads_the_database() {
        use dbcore::*;
        use std::sync::{Arc, LazyLock, atomic::AtomicBool};
        use tauri::Manager;
        struct Offline;
        #[async_trait::async_trait]
        impl Session for Offline {
            async fn ping(&self) -> Result<(), DbError> { panic!("lint pinged database") }
            async fn server_info(&self) -> Result<String, DbError> { panic!("lint read server info") }
            async fn list_children(&self, _: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> { panic!("lint read catalog") }
            async fn browse(&self, _: &str, _: PageRequest) -> Result<ResultSet, DbError> { panic!("lint browsed data") }
            async fn execute(&self, _: &ExecRequest, _: CancelToken) -> Result<Vec<ResultSet>, DbError> { panic!("lint executed metadata query") }
            async fn explain(&self, _: &str, _: bool, _: CancelToken) -> Result<PlanTree, DbError> { panic!("lint executed EXPLAIN") }
            async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> { panic!("lint expanded data") }
            async fn close(&self) {}
        }
        let dir = tempfile::tempdir().unwrap();
        let mut host = Host::new(crate::storage::Paths {config:dir.path().into(),data:dir.path().into(),log:dir.path().into()});
        host.registry = LazyLock::new(Box::new(|| {
            let registry = Registry::new();
            registry.register(Arc::new(driver_postgres::PostgresDriver::new()));
            registry
        }));
        host.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert("offline".into(), Arc::new(crate::SessionRecord {
            session: Arc::new(Offline),
            profile: crate::storage::Profile { id:"p".into(),name:"test".into(),driver_id:"postgres".into(),fields:Default::default(),save_secret:false,network:None,auth:Default::default(),access_mode:Default::default() },
            secret:None,_tunnel:None,dead:AtomicBool::new(false),
        }));
        let app = tauri::test::mock_builder().manage(host).build(tauri::test::mock_context(tauri::test::noop_assets())).unwrap();
        let diagnostics = lint_query(app.state(), "offline".into(), "SELECT * FROM people".into()).await.unwrap();
        assert!(diagnostics.iter().any(|d| d.rule_id == "select_star"));
        assert!(diagnostics.iter().all(|d| d.quick_fix.is_none()));
        assert!(lint_query(app.state(), "offline".into(), "SELECT 1".into()).await.unwrap().is_empty());
    }

    fn schema() -> SchemaGraph {
        SchemaGraph {
            entities: vec![dbcore::SchemaEntity {
                id: "people".into(),
                name: "people".into(),
                kind: "table".into(),
                fields: vec![
                    dbcore::SchemaField {
                        name: "id".into(),
                        type_name: "integer".into(),
                        pk: true,
                        nullable: false,
                    },
                    dbcore::SchemaField {
                        name: "이름".into(),
                        type_name: "text".into(),
                        pk: false,
                        nullable: true,
                    },
                ],
            }],
            relations: vec![],
        }
    }
    fn apply(text: &str, edits: &[TextEdit]) -> String {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        for e in edits.iter().rev() {
            units.splice(e.start..e.end, e.text.encode_utf16());
        }
        String::from_utf16(&units).unwrap()
    }
    #[test]
    fn lint_walks_ctes_and_preserves_literal_boundaries() {
        let issues = lint(
            "WITH x AS (DELETE FROM people RETURNING *) SELECT * FROM x",
            "sql",
        )
        .unwrap();
        assert!(issues.iter().any(|d| d.rule_id == "missing_where"));
        assert!(!lint("UPDATE people SET 이름 = 'WHERE'", "sql")
            .unwrap()
            .is_empty());
        assert!(!lint("SELECT a.id FROM people a, people b", "sql")
            .unwrap()
            .is_empty());
        assert!(lint("SELECT '*' FROM people", "sql").unwrap().is_empty());
        assert!(lint("MATCH (a)-[:KNOWS*]->(b) RETURN a", "cypher")
            .unwrap()
            .iter()
            .any(|d| d.rule_id == "cypher_unbounded_path"));
        assert!(lint("MATCH (a)-[:KNOWS*1..5]->(b) RETURN a", "cypher")
            .unwrap()
            .is_empty());
        assert!(lint("RETURN '*', '[r*]'", "cypher").unwrap().is_empty());
        assert!(
            lint("// MATCH (a)-[:KNOWS*]->(b) RETURN 'unfinished", "cypher")
                .unwrap()
                .is_empty()
        );
        assert!(lint(
            r#"RETURN 'escaped\' [r*]', "😀[r*]", `column[r*]` /* [r*] */"#,
            "cypher"
        )
        .unwrap()
        .is_empty());
        let query = "// [r*]\nMATCH (a)-[:KNOWS*]->(b) RETURN a";
        let issues = lint(query, "cypher").unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(
            apply(query, &issues[0].quick_fix.as_ref().unwrap().edits),
            "// [r*]\nMATCH (a)-[:KNOWS*1..5]->(b) RETURN a"
        );
        assert_eq!(
            lint("MATCH (a)-->(b)-[:KNOWS*]->(c) RETURN c", "cypher")
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn refactors_use_original_utf16_spans_and_do_not_touch_strings() {
        let text = "SELECT '😀', p.* FROM people AS p WHERE p.id = 1";
        let edits = refactor(text, "sql", RefactorAction::ExpandStar, &schema()).unwrap();
        assert_eq!(
            apply(text, &edits),
            "SELECT '😀', p.\"id\", p.\"이름\" FROM people AS p WHERE p.id = 1"
        );
        let edits = refactor(
            text,
            "sql",
            RefactorAction::RenameAlias {
                from: "p".into(),
                to: "person".into(),
            },
            &schema(),
        )
        .unwrap();
        assert_eq!(
            apply(text, &edits),
            "SELECT '😀', \"person\".* FROM people AS \"person\" WHERE \"person\".id = 1"
        );
        let text = "SELECT t.id FROM (SELECT id FROM people) AS t";
        let edits = refactor(
            text,
            "sql",
            RefactorAction::ExtractCte {
                name: "source".into(),
            },
            &schema(),
        )
        .unwrap();
        let result = apply(text, &edits);
        assert_eq!(
            result,
            "WITH \"source\" AS (SELECT id FROM people)\nSELECT t.id FROM \"source\" AS t"
        );
        assert!(parse(&result, "sql").is_ok());
        assert!(refactor(
            "SELECT * FROM people p JOIN people q ON p.id=q.id",
            "sql",
            RefactorAction::ExpandStar,
            &schema()
        )
        .is_err());
        assert!(refactor(
            "SELECT p.id FROM people p WHERE EXISTS(SELECT 1)",
            "sql",
            RefactorAction::RenameAlias {
                from: "p".into(),
                to: "q".into()
            },
            &schema()
        )
        .is_err());
        assert!(refactor(
            "SELECT 1 -- keep comment",
            "sql",
            RefactorAction::Format,
            &schema()
        )
        .is_err());
    }
    #[test]
    fn formatting_is_parseable_and_rejects_injection_identifiers_by_quoting() {
        let text = "select id,name from people where id=1;";
        let edits = refactor(text, "sql", RefactorAction::Format, &schema()).unwrap();
        assert_eq!(
            parse(text, "sql").unwrap()[0].to_string(),
            parse(&apply(text, &edits), "sql").unwrap()[0].to_string()
        );
        assert_eq!(
            identifier("x\"; DROP TABLE people;--").unwrap(),
            "\"x\"\"; DROP TABLE people;--\""
        );
    }
}
