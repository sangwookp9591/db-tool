use super::*;
impl RedisSession {
    pub(super) async fn native_roles(&self) -> Result<RolesSnapshot, DbError> {
        let mut connection = self.connection().await?;
        let users: Vec<String> = redis::cmd("ACL")
            .arg("USERS")
            .query_async(&mut connection)
            .await
            .map_err(error)?;
        if users.len() > 1000 {
            return Err(DbError::query("ACL users exceed 1000"));
        }
        let mut snapshot = RolesSnapshot {
            supported: true,
            ..Default::default()
        };
        for user in users {
            let values: Vec<(String, Resp)> = redis::cmd("ACL")
                .arg("GETUSER")
                .arg(&user)
                .query_async(&mut connection)
                .await
                .map_err(error)?;
            let mut summary = vec![];
            // Password hashes from GETUSER are deliberately discarded before serialization.
            for (key, value) in values {
                if ["commands", "keys", "channels", "flags", "selectors"].contains(&key.as_str()) {
                    let text = match value {
                        Resp::BulkString(s) => String::from_utf8_lossy(&s).into_owned(),
                        Resp::Array(a) => a
                            .into_iter()
                            .filter_map(|v| redis::from_redis_value::<String>(v).ok())
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    };
                    summary.push(format!("{key}: {text}"));
                    if key == "commands" || key == "keys" {
                        snapshot.matrix.push(ObjectPrivilege {
                            role: user.clone(),
                            namespace: "ACL".into(),
                            object: key,
                            privilege: text,
                            allowed: true,
                        });
                    }
                }
            }
            snapshot.roles.push(RoleInfo {
                name: user,
                login: true,
                superuser: false,
                member_of: vec![],
                summary,
            });
        }
        Ok(snapshot)
    }
    pub(super) fn native_grants(&self, changes: &[GrantChange]) -> Result<GrantPlan, DbError> {
        validate_grants(changes)?;
        let mut statements = vec![];
        for c in changes {
            if c.namespace != "command"
                || !c.object.is_empty()
                || c.role.bytes().any(|b| b.is_ascii_whitespace())
                || c.role.contains(['"', '\'', '\\'])
                || c.privilege.is_empty()
                || !c
                    .privilege
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"@_|-".contains(&b))
            {
                return Err(DbError::query("ACL plans require namespace command, empty object, and one command/category token"));
            }
            statements.push(format!(
                "ACL SETUSER {} {}{}",
                c.role,
                if c.revoke { "-" } else { "+" },
                c.privilege
            ));
        }
        Ok(GrantPlan{statements,warnings:vec!["ACL plans are inspection only; apply in your Redis administration workflow. Existing selectors and key/channel patterns affect effective access.".into()],apply_supported:false})
    }
    pub(super) async fn import_batch(
        &self,
        req: &EditRequest,
        preview: bool,
        cancel: CancelToken,
    ) -> Result<ImportOutput, DbError> {
        if req.create_table.is_some() || req.edits.is_empty() || req.edits.len() > IMPORT_MAX_ROWS {
            return Err(DbError::query(
                "Redis import requires 1..10000 key/value rows",
            ));
        }
        let path = parse_path(&req.node_id)?;
        let (prefix, _, _) = scan_path(&path)?;
        let mut keys = vec![];
        let mut values = vec![];
        let mut unique = BTreeSet::new();
        let mut bytes = 0;
        for edit in &req.edits {
            let RowEdit::Insert { values: fields } = edit else {
                return Err(DbError::query("Import supports inserts only"));
            };
            if fields.len() != 2 {
                return Err(DbError::query(
                    "Redis import requires exactly key and value columns",
                ));
            }
            let field = |name: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k == name)
                    .ok_or_else(|| DbError::query("Redis import requires key and value columns"))
            };
            let mut key = prefix.clone();
            key.extend_from_slice(
                dbcore::edits::text(&field("key")?.1)?
                    .ok_or_else(|| DbError::query("Key must not be null"))?
                    .as_bytes(),
            );
            let value = dbcore::edits::text(&field("value")?.1)?
                .ok_or_else(|| DbError::query("Value must not be null"))?;
            bytes += key.len() + value.len();
            if bytes > 8 * 1024 * 1024 || !unique.insert(key.clone()) {
                return Err(DbError::query("Duplicate key or import exceeds 8 MiB"));
            }
            keys.push(key);
            values.push(value);
        }
        self.guard(req.timeout_ms,cancel.clone(),async {
            let mut connection=self.connection().await?;
            // MSETNX validates the complete key set and inserts all rows atomically.
            let count = if preview {
                let existing:u64=redis::cmd("EXISTS").arg(&keys).query_async(&mut connection).await.map_err(error)?;
                if existing>0{return Err(DbError::new(ErrorKind::Conflict,"Import target key already exists"));}
                keys.len() as u64
            } else {
                let mut command=redis::cmd("MSETNX");
                for (key,value) in keys.iter().zip(&values){command.arg(key).arg(value);}
                let inserted:bool=command.query_async(&mut connection).await.map_err(error)?;
                if !inserted{return Err(DbError::new(ErrorKind::Conflict,"Import target key already exists"));}
                keys.len() as u64
            };
            let text=format!("MSET {count} new keys (values omitted)");
            if preview {let mut statement=StatementPreview::new(text,StatementKind::Insert);statement.affected=Some(count);Ok(ImportOutput::Preview(ChangePreview{statements:vec![statement],committed:false,notes:vec!["Dry run validates missing keys without writing; apply atomically rechecks all keys".into()]}))}else{Ok(ImportOutput::Applied(EditResult{affected:vec![count],statements:vec![text]}))}
        }).await
    }
}
