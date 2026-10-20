use super::*;
fn bson(value: &Value) -> Result<Bson, DbError> {
    Ok(match value {
        Value::Null => Bson::Null,
        Value::Bool(v) => Bson::Boolean(*v),
        Value::Int(v) => Bson::Int64(v.parse().map_err(|_| DbError::query("Invalid integer"))?),
        Value::Float(FloatValue::Number(v)) if v.is_finite() => Bson::Double(*v),
        Value::Text(v) => Bson::String(v.clone()),
        Value::Decimal(v) => Bson::Decimal128(
            v.parse()
                .map_err(|_| DbError::query("Invalid Decimal128"))?,
        ),
        Value::DateTimeTz(v) => Bson::DateTime(
            mongodb::bson::DateTime::parse_rfc3339_str(v)
                .map_err(|_| DbError::query("Invalid date"))?,
        ),
        Value::Other { type_name, text } if type_name == "ObjectId" => Bson::ObjectId(
            mongodb::bson::oid::ObjectId::parse_str(text)
                .map_err(|_| DbError::query("Invalid ObjectId"))?,
        ),
        Value::List(v) => Bson::Array(v.iter().map(bson).collect::<Result<_, _>>()?),
        Value::Map(v) if v.iter().all(|(name, _)| !name.starts_with('$')) => Bson::Document(
            v.iter()
                .map(|(k, v)| Ok((k.clone(), bson(v)?)))
                .collect::<Result<_, DbError>>()?,
        ),
        Value::Json(v) => Bson::try_from(
            serde_json::from_str::<serde_json::Value>(v)
                .map_err(|_| DbError::query("Invalid JSON"))?,
        )
        .map_err(|_| DbError::query("Invalid Extended JSON"))?,
        _ => return Err(DbError::unsupported("이 BSON 값은 편집할 수 없습니다")),
    })
}
fn field(name: &str) -> bool {
    !name.is_empty() && !name.contains(['.', '\0']) && !name.starts_with('$')
}
impl MongoSession {
    pub(super) async fn find_target(
        &self,
        collection: &str,
        documents: &[Document],
    ) -> Result<EditTarget, &'static str> {
        let mut cursor = self
            .db()
            .list_collections()
            .filter(doc! {"name":collection})
            .await
            .map_err(|_| "컬렉션을 확인할 수 없습니다")?;
        if !cursor
            .advance()
            .await
            .map_err(|_| "컬렉션을 확인할 수 없습니다")?
            || cursor
                .deserialize_current()
                .map_err(|_| "컬렉션을 확인할 수 없습니다")?
                .collection_type
                != mongodb::results::CollectionType::Collection
        {
            return Err("뷰 결과는 편집할 수 없습니다");
        }
        if documents.is_empty() || documents.iter().any(|d| !d.contains_key("_id")) {
            return Err("기본 키가 결과에 없습니다");
        }
        let table = document_result(documents.to_vec(), false).table.unwrap();
        let origins = table
            .columns
            .iter()
            .map(|c| {
                (field(&c.name)
                    && documents
                        .iter()
                        .all(|d| d.get(&c.name).is_some_and(|v| bson(&map_value(v)).is_ok())))
                .then(|| c.name.clone())
            })
            .collect();
        dbcore::query_edit::target(
            serde_json::json!(["collection", self.database, collection]).to_string(),
            vec!["_id".into()],
            origins,
        )
    }
    pub(super) async fn edit_document(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
        preview: bool,
    ) -> Result<(EditResult, ChangePreview), DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session is closed"));
        }
        // shortcut: one atomic updateOne per approval supports standalone MongoDB; batch editing needs replica-set transactions.
        if req.edits.len() != 1 || req.create_table.is_some() || req.originals.len() != 1 {
            return Err(DbError::unsupported(
                "MongoDB는 한 번에 문서 하나의 필드 수정만 지원합니다",
            ));
        }
        let path = parse_path(&req.node_id)?;
        if path.len() != 3 || path[0] != "collection" {
            return Err(DbError::query("Select a collection"));
        }
        let RowEdit::Update { key, set } = &req.edits[0] else {
            return Err(DbError::unsupported(
                "MongoDB 결과는 필드 수정만 지원합니다",
            ));
        };
        if key.len() != 1 || key[0].0 != "_id" || set.is_empty() {
            return Err(DbError::query("_id and updated fields are required"));
        }
        let original = &req.originals[0];
        if set
            .iter()
            .any(|(n, _)| n == "_id" || !original.iter().any(|(o, _)| o == n))
        {
            return Err(DbError::query("원래 값이 있는 필드만 수정할 수 있습니다"));
        }
        let mut conditions = vec![Bson::Document(doc! {"_id":{"$eq":bson(&key[0].1)?}})];
        let mut seen = BTreeSet::new();
        for (name, value) in original {
            if !field(name) || !seen.insert(name) {
                return Err(DbError::query("Invalid original field"));
            }
            conditions.push(Bson::Document(
                doc! {name:{"$eq":bson(value)?,"$exists":true}},
            ));
        }
        let mut updates = Document::new();
        for (name, value) in set {
            if !field(name) || updates.contains_key(name) {
                return Err(DbError::query("Invalid or duplicate field"));
            }
            updates.insert(name, bson(value)?);
        }
        let filter = doc! {"$and":conditions};
        let update = doc! {"$set":updates};
        let text=serde_json::json!({"updateOne":{"collection":path[2],"filter":Bson::Document(filter.clone()).into_canonical_extjson(),"update":Bson::Document(update.clone()).into_canonical_extjson()}}).to_string();
        let db = self.client.database(&path[1]);
        let collection = db.collection::<Document>(&path[2]);
        let work = async {
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "편집이 취소되었습니다"));
            }
            let before = collection
                .find_one(filter.clone())
                .await
                .map_err(|e| self.error(e))?
                .ok_or_else(|| {
                    DbError::new(
                        ErrorKind::Conflict,
                        "문서가 변경되었거나 삭제되었습니다. 다시 조회하세요",
                    )
                })?;
            let mut after = before.clone();
            for (name, value) in update.get_document("$set").unwrap() {
                after.insert(name, value.clone());
            }
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "편집이 취소되었습니다"));
            }
            if !preview {
                let outcome = collection
                    .update_one(filter, update)
                    .await
                    .map_err(|e| self.error(e))?;
                if outcome.matched_count != 1 {
                    return Err(DbError::new(
                        ErrorKind::Conflict,
                        "문서가 변경되었습니다. 다시 조회하세요",
                    ));
                }
            }
            let mut statement = StatementPreview::new(text.clone(), StatementKind::Update);
            statement.affected = Some(1);
            statement.before = document_result(vec![before], false).table;
            statement.after = document_result(vec![after], false).table;
            Ok((EditResult{affected:vec![1],statements:vec![text]},ChangePreview{statements:vec![statement],committed:false,notes:vec!["MongoDB 미리보기는 쓰기 없이 변경 전·후를 계산합니다. 적용은 원래 값이 일치할 때만 atomic updateOne으로 실행합니다. 한 번에 문서 하나를 수정하세요.".into()]}))
        };
        // The host cancels then drains this future, preserving the outcome of an in-flight atomic write.
        work.await
    }
}
