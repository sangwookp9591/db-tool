use super::*;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct CollectionDump {
    name: String,
    options: serde_json::Value,
    indexes: Vec<serde_json::Value>,
    ndjson: String,
}
impl MongoSession {
    fn ensure_admin_open(&self) -> Result<(), DbError> {
        if self.closed.load(Ordering::Acquire) {
            Err(DbError::new(ErrorKind::Connection, "Session closed"))
        } else {
            Ok(())
        }
    }

    pub(super) async fn native_backup(&self, cancel: CancelToken) -> Result<BackupData, DbError> {
        self.ensure_admin_open()?;
        let mut collections = self
            .db()
            .list_collections()
            .await
            .map_err(|e| self.error(e))?;
        let mut output = vec![];
        let mut counts = vec![];
        let mut bytes = 0;
        while collections.advance().await.map_err(|e| self.error(e))? {
            cancelled(&cancel)?;
            let spec = collections
                .deserialize_current()
                .map_err(|e| self.error(e))?;
            if spec.name.starts_with("system.") {
                continue;
            }
            let collection = self.db().collection::<Document>(&spec.name);
            let mut cursor = collection.find(doc! {}).await.map_err(|e| self.error(e))?;
            let mut ndjson = String::new();
            let mut count = 0;
            while cursor.advance().await.map_err(|e| self.error(e))? {
                cancelled(&cancel)?;
                let doc = cursor.deserialize_current().map_err(|e| self.error(e))?;
                let line = Bson::Document(doc).into_canonical_extjson().to_string();
                bytes += line.len() + 1;
                if bytes > BACKUP_MAX_BYTES {
                    return Err(DbError::query("Backup exceeds 8 MiB"));
                }
                ndjson.push_str(&line);
                ndjson.push('\n');
                count += 1;
            }
            let mut cursor = collection.list_indexes().await.map_err(|e| self.error(e))?;
            let mut indexes = vec![];
            while cursor.advance().await.map_err(|e| self.error(e))? {
                let index = cursor.deserialize_current().map_err(|e| self.error(e))?;
                if index.options.as_ref().and_then(|o| o.name.as_deref()) != Some("_id_") {
                    indexes.push(
                        serde_json::to_value(index).map_err(|_| DbError::query("Invalid index"))?,
                    );
                }
            }
            counts.push((spec.name.clone(), count));
            output.push(CollectionDump {
                name: spec.name,
                options: serde_json::to_value(spec.options)
                    .map_err(|_| DbError::query("Invalid collection options"))?,
                indexes,
                ndjson,
            });
        }
        let backup = BackupData {
            format: "mongodb-ndjson-v1".into(),
            data: serde_json::to_vec(&output)
                .map_err(|_| DbError::query("Cannot encode backup"))?,
            counts,
        };
        backup.validate("mongodb-ndjson-v1")?;
        Ok(backup)
    }
    pub(super) async fn native_restore(
        &self,
        backup: &BackupData,
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        self.ensure_admin_open()?;
        backup.validate("mongodb-ndjson-v1")?;
        if overwrite {
            return Err(DbError::unsupported("MongoDB overwrite restore is disabled until atomic database replacement is available"));
        }
        if !self
            .db()
            .list_collection_names()
            .await
            .map_err(|e| self.error(e))?
            .is_empty()
        {
            return Err(DbError::new(
                ErrorKind::Conflict,
                "Restore requires an empty database",
            ));
        }
        let dump: Vec<CollectionDump> = serde_json::from_slice(&backup.data)
            .map_err(|_| DbError::query("Invalid MongoDB backup"))?;
        let mut names = std::collections::HashSet::new();
        for c in &dump {
            if c.name.is_empty()
                || c.name.starts_with("system.")
                || c.name.contains('\0')
                || !names.insert(&c.name)
            {
                return Err(DbError::query("Invalid or duplicate collection"));
            }
        }
        let mut created = vec![];
        let result = async {
            for c in dump {
                cancelled(&cancel)?;
                let options: mongodb::options::CreateCollectionOptions =
                    serde_json::from_value(c.options)
                        .map_err(|_| DbError::query("Invalid collection options"))?;
                self.db()
                    .create_collection(&c.name)
                    .with_options(options)
                    .await
                    .map_err(|e| self.error(e))?;
                created.push(c.name.clone());
                let collection = self.db().collection::<Document>(&c.name);
                let mut batch = vec![];
                for line in c.ndjson.lines() {
                    cancelled(&cancel)?;
                    let json = serde_json::from_str::<serde_json::Value>(line)
                        .map_err(|_| DbError::query("Invalid NDJSON"))?;
                    let Bson::Document(doc) = Bson::try_from(json)
                        .map_err(|_| DbError::query("Invalid Extended JSON"))?
                    else {
                        return Err(DbError::query("Expected document"));
                    };
                    batch.push(doc);
                    if batch.len() == 500 {
                        collection
                            .insert_many(std::mem::take(&mut batch))
                            .await
                            .map_err(|e| self.error(e))?;
                    }
                }
                if !batch.is_empty() {
                    collection
                        .insert_many(batch)
                        .await
                        .map_err(|e| self.error(e))?;
                }
                if !c.indexes.is_empty() {
                    let indexes = c
                        .indexes
                        .into_iter()
                        .map(serde_json::from_value::<mongodb::IndexModel>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| DbError::query("Invalid index metadata"))?;
                    collection
                        .create_indexes(indexes)
                        .await
                        .map_err(|e| self.error(e))?;
                }
            }
            cancelled(&cancel)
        }
        .await;
        if result.is_err() {
            for name in created {
                if self
                    .db()
                    .collection::<Document>(&name)
                    .drop()
                    .await
                    .is_err()
                {
                    return Err(DbError::new(
                        ErrorKind::Connection,
                        "Restore failed and cleanup could not be confirmed",
                    ));
                }
            }
        }
        result
    }
    pub(super) async fn native_roles(&self) -> Result<RolesSnapshot, DbError> {
        self.ensure_admin_open()?;
        let users = self
            .db()
            .run_command(doc! {"usersInfo":1,"showCredentials":false})
            .await
            .map_err(|e| self.error(e))?;
        let mut detailed = vec![];
        if let Ok(entries) = users.get_array("users") {
            for entry in entries {
                if let Some(user) = entry.as_document() {
                    let name = user
                        .get_str("user")
                        .map_err(|_| DbError::query("Invalid user metadata"))?;
                    let db = user.get_str("db").unwrap_or(&self.database);
                    let detail=self.db().run_command(doc!{"usersInfo":{"user":name,"db":db},"showPrivileges":true,"showCredentials":false}).await.map_err(|e|self.error(e))?;
                    if let Ok(users) = detail.get_array("users") {
                        detailed.extend(users.clone());
                    }
                }
            }
        }
        let users = doc! {"users":detailed};
        let roles = self
            .db()
            .run_command(doc! {"rolesInfo":1,"showPrivileges":true,"showBuiltinRoles":true})
            .await
            .map_err(|e| self.error(e))?;
        let mut snapshot = RolesSnapshot {
            supported: true,
            ..Default::default()
        };
        for (docs, key, login) in [(&users, "users", true), (&roles, "roles", false)] {
            if let Ok(entries) = docs.get_array(key) {
                for entry in entries {
                    let Some(doc) = entry.as_document() else {
                        continue;
                    };
                    let name = doc
                        .get_str(if login { "user" } else { "role" })
                        .unwrap_or("")
                        .to_owned();
                    let member_of = doc
                        .get_array("roles")
                        .map(|rs| {
                            rs.iter()
                                .filter_map(|r| {
                                    r.as_document()?.get_str("role").ok().map(str::to_owned)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Ok(privileges) = doc.get_array("inheritedPrivileges") {
                        for privilege in privileges {
                            let Some(p) = privilege.as_document() else {
                                continue;
                            };
                            let resource = p.get_document("resource").ok();
                            let namespace =
                                resource.and_then(|r| r.get_str("db").ok()).unwrap_or("*");
                            let object = resource
                                .and_then(|r| r.get_str("collection").ok())
                                .unwrap_or("*");
                            if let Ok(actions) = p.get_array("actions") {
                                for action in actions {
                                    if let Some(action) = action.as_str() {
                                        snapshot.matrix.push(ObjectPrivilege {
                                            role: name.clone(),
                                            namespace: namespace.into(),
                                            object: object.into(),
                                            privilege: action.into(),
                                            allowed: true,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    snapshot.roles.push(RoleInfo{name,login,superuser:false,member_of,summary:vec!["Effective inherited privileges are listed in the matrix; wildcard resources may cover additional objects".into()]});
                }
            }
        }
        if snapshot.roles.len() > 1000 || snapshot.matrix.len() > 50000 {
            return Err(DbError::query("Roles exceed inspection limit"));
        }
        Ok(snapshot)
    }
    pub(super) fn native_grants(&self, changes: &[GrantChange]) -> Result<GrantPlan, DbError> {
        validate_grants(changes)?;
        if changes.len() != 1 {
            return Err(DbError::unsupported(
                "MongoDB role membership changes apply one at a time",
            ));
        }
        let c = &changes[0];
        if c.privilege != "ROLE" || c.object.is_empty() || c.namespace.is_empty() {
            return Err(DbError::query(
                "MongoDB grants require ROLE, a role name in object, and its database in namespace",
            ));
        }
        let command = if c.revoke {
            "revokeRolesFromUser"
        } else {
            "grantRolesToUser"
        };
        Ok(GrantPlan {
            statements: vec![
                serde_json::json!({command:c.role,"roles":[{"role":c.object,"db":c.namespace}]})
                    .to_string(),
            ],
            warnings: vec![
                "Changes user role membership in the current authentication database".into(),
            ],
            apply_supported: true,
        })
    }
    pub(super) async fn native_apply_grants(
        &self,
        changes: &[GrantChange],
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        self.ensure_admin_open()?;
        let plan = self.native_grants(changes)?;
        cancelled(&cancel)?;
        let json: serde_json::Value = serde_json::from_str(&plan.statements[0])
            .map_err(|_| DbError::query("Invalid grant plan"))?;
        let Bson::Document(command) =
            Bson::try_from(json).map_err(|_| DbError::query("Invalid grant command"))?
        else {
            return Err(DbError::query("Invalid grant command"));
        };
        self.db()
            .run_command(command)
            .await
            .map_err(|e| self.error(e))?;
        Ok(())
    }
}
fn cancelled(cancel: &CancelToken) -> Result<(), DbError> {
    if cancel.is_cancelled() {
        Err(DbError::new(ErrorKind::Cancelled, "Operation cancelled"))
    } else {
        Ok(())
    }
}
