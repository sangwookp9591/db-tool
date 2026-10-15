use dbcore::DbError;

/// Split PostgreSQL statements without treating quoted/commented semicolons as delimiters.
pub fn split_statements(sql: &str) -> Result<Vec<String>, DbError> {
    let b = sql.as_bytes();
    let (mut i, mut start) = (0, 0);
    let mut statements = Vec::new();
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' => {
                let quote = b[i];
                let escape = quote == b'\''
                    && i > 0
                    && matches!(b[i - 1], b'e' | b'E')
                    && (i < 2 || !ident(b[i - 2]));
                i += 1;
                let mut closed = false;
                while i < b.len() {
                    if escape && b[i] == b'\\' {
                        i = (i + 2).min(b.len());
                    } else if b[i] == quote {
                        i += 1;
                        if i < b.len() && b[i] == quote {
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
                    return Err(DbError::query("Unterminated quoted string or identifier"));
                }
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut depth = 1;
                while i < b.len() && depth > 0 {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if depth > 0 {
                    return Err(DbError::query("Unterminated block comment"));
                }
            }
            b'$' if i == 0 || !ident(b[i - 1]) => {
                let mut j = i + 1;
                if b.get(j) != Some(&b'$') && b.get(j).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                    continue;
                }
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if b.get(j) == Some(&b'$') {
                    let tag = &sql[i..=j];
                    if let Some(end) = sql[j + 1..].find(tag) {
                        i = j + 1 + end + tag.len();
                    } else {
                        return Err(DbError::query("Unterminated dollar-quoted string"));
                    }
                } else {
                    i += 1;
                }
            }
            b';' => {
                let statement = sql[start..i].trim();
                if !keywords(statement).is_empty() {
                    statements.push(statement.to_string());
                }
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    let tail = sql[start..].trim();
    if !keywords(tail).is_empty() {
        statements.push(tail.into());
    }
    Ok(statements)
}
fn ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 128
}

/// The leading words, ignoring whitespace and nested comments, for transaction control.
pub fn keywords(sql: &str) -> Vec<String> {
    let b = sql.as_bytes();
    let mut i = 0;
    let mut words = Vec::new();
    while i < b.len() && words.len() < 3 {
        if b[i].is_ascii_whitespace() {
            i += 1;
        } else if b[i..].starts_with(b"--") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            let start = i;
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            if start == i {
                break;
            }
            words.push(sql[start..i].to_ascii_uppercase());
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn postgres_quotes_comments_and_unicode() {
        let sql = "SELECT ';', 'it''s;', \"a;b\", $$x;y$$, $tag$x;y$tag$, E'a\\\';b'; -- ;\n /* ; /* nested ; */ ; */ SELECT '한글';";
        let parts = split_statements(sql).unwrap();
        assert_eq!(parts.len(), 2);
        assert!(parts[1].contains("한글"));
        assert_eq!(keywords(&parts[1]), ["SELECT"]);
        assert_eq!(
            split_statements("-- only;\n /* comment */").unwrap().len(),
            0
        );
        assert_eq!(split_statements("SELECT $1; SELECT 2").unwrap().len(), 2);
    }
    #[test]
    fn rejects_unterminated_input_before_any_execution() {
        for text in [
            "SELECT 1; SELECT 'x",
            "SELECT $$x",
            "SELECT /* x",
            "SELECT \"x",
        ] {
            assert!(split_statements(text).is_err(), "{text}");
        }
    }
}
