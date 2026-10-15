use crate::{value::type_name, NeoSession};
use dbcore::*;
use std::collections::{BTreeMap, BTreeSet};
fn ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}
fn path(kind: &str, name: &str) -> String {
    serde_json::to_string(&[kind, name]).unwrap()
}
fn parse(id: &str) -> Result<[String; 2], DbError> {
    serde_json::from_str(id).map_err(|_| DbError::query("Invalid explorer node"))
}
fn node(kind: &str, name: &str, icon: Icon, children: bool) -> ExplorerNode {
    ExplorerNode {
        id: path(kind, name),
        label: name.into(),
        detail: None,
        kind: kind.into(),
        icon,
        has_children: children,
        actions: vec![],
    }
}
fn cell<'a>(table: &'a Table, row: &'a [Value], name: &str) -> Option<&'a Value> {
    table
        .columns
        .iter()
        .position(|c| c.name == name)
        .and_then(|i| row.get(i))
}
fn text(v: &Value) -> Option<&str> {
    if let Value::Text(s) = v {
        Some(s)
    } else {
        None
    }
}
fn browse_query(kind: &str, name: &str) -> Result<String, DbError> {
    match kind {
        "label" => Ok(format!("MATCH (n:{}) RETURN n", ident(name))),
        "rel_type" => Ok(format!("MATCH p=()-[r:{}]->() RETURN p", ident(name))),
        _ => Err(DbError::query("Node cannot be browsed")),
    }
}
impl NeoSession {
    pub(crate) async fn explore(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        let Some(parent) = parent else {
            return Ok([
                ("labels", "Labels"),
                ("relationships", "Relationship Types"),
                ("properties", "Property Keys"),
                ("schema", "Indexes & Constraints"),
            ]
            .into_iter()
            .map(|(kind, name)| node(kind, name, Icon::Folder, true))
            .collect());
        };
        let [kind, name] = parse(parent)?;
        if kind == "label" {
            return self.label_properties(&name).await;
        }
        let queries:Vec<(&str,&str,Icon,bool,&str)>=match kind.as_str() {
            "labels"=>vec![("CALL db.labels() YIELD label RETURN label ORDER BY label","label",Icon::Label,true,"label")],
            "relationships"=>vec![("CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType ORDER BY relationshipType","rel_type",Icon::Relationship,false,"relationshipType")],
            "properties"=>vec![("CALL db.propertyKeys() YIELD propertyKey RETURN propertyKey ORDER BY propertyKey","property",Icon::Property,false,"propertyKey")],
            "schema"=>vec![("SHOW INDEXES","index",Icon::Index,false,"name"),("SHOW CONSTRAINTS","constraint",Icon::Key,false,"name")],
            _=>return Ok(vec![]),
        };
        let mut nodes = Vec::new();
        for (query, kind, icon, children, key) in queries {
            let result = self.internal(query.into(), Default::default()).await?;
            if let Some(table) = result.table {
                for row in &table.rows {
                    if let Some(name) = cell(&table, row, key).and_then(text) {
                        let mut n = node(kind, name, icon.clone(), children);
                        if matches!(kind, "label" | "rel_type") {
                            n.actions = vec![
                                NodeAction {
                                    id: "browse".into(),
                                    label: "Browse".into(),
                                    kind: NodeActionKind::Browse,
                                },
                                NodeAction {
                                    id: "query".into(),
                                    label: "Open query".into(),
                                    kind: NodeActionKind::OpenQuery {
                                        text: format!("{} LIMIT 25", browse_query(kind, name)?),
                                    },
                                },
                            ];
                        }
                        nodes.push(n);
                    }
                }
            }
        }
        Ok(nodes)
    }
    async fn label_properties(&self, label: &str) -> Result<Vec<ExplorerNode>, DbError> {
        let schema=self.internal("CALL db.schema.nodeTypeProperties() YIELD nodeLabels, propertyName, propertyTypes WHERE $label IN nodeLabels RETURN propertyName, propertyTypes".into(),serde_json::json!({"label":label}).as_object().unwrap().clone()).await;
        let mut properties: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        match schema {
            Ok(result) => {
                if let Some(table) = result.table {
                    for row in &table.rows {
                        if let Some(name) = cell(&table, row, "propertyName").and_then(text) {
                            if let Some(Value::List(types)) = cell(&table, row, "propertyTypes") {
                                for typ in types {
                                    if let Some(typ) = text(typ) {
                                        properties
                                            .entry(name.into())
                                            .or_default()
                                            .insert(typ.into());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                let sample = self
                    .internal(
                        format!("MATCH (n:{}) RETURN n LIMIT 100", ident(label)),
                        Default::default(),
                    )
                    .await?;
                if let Some(table) = sample.table {
                    for row in table.rows {
                        for value in row {
                            if let Value::Node(n) = value {
                                for (key, v) in n.props {
                                    properties
                                        .entry(key)
                                        .or_default()
                                        .insert(type_name(&v).into());
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(properties
            .into_iter()
            .map(|(key, types)| {
                let mut n = node("property", &key, Icon::Property, false);
                n.id = serde_json::to_string(&["property", label, &key]).unwrap();
                n.detail = Some(types.into_iter().collect::<Vec<_>>().join(" | "));
                n
            })
            .collect())
    }
    pub(crate) async fn browse_node(
        &self,
        id: &str,
        page: PageRequest,
    ) -> Result<ResultSet, DbError> {
        let [kind, name] = parse(id)?;
        let limit = clamp_row_limit(page.limit);
        let base = browse_query(&kind, &name)?;
        let order = if kind == "label" { "id(n)" } else { "id(r)" };
        let params=serde_json::json!({"o":i64::try_from(page.offset).map_err(|_|DbError::query("Page offset exceeds Bolt integer range"))?,"l":limit+1}).as_object().unwrap().clone();
        let mut result = self
            .bounded(
                &ExecRequest {
                    text: format!("{base} ORDER BY {order} SKIP $o LIMIT $l"),
                    read_only: true,
                    params,
                    row_limit: limit + 1,
                    graph_node_limit: 300,
                    timeout_ms: 30_000,
                    preview_sample_limit: default_preview_sample_limit(),
                },
                CancelToken::new(),
                true,
            )
            .await?;
        let has_more = result.row_count > u64::from(limit);
        if let Some(table) = &mut result.table {
            table.rows.truncate(limit as usize);
            let mut collector = crate::value::Collector::default();
            for row in &table.rows {
                for v in row {
                    collector.collect(v, 300);
                }
            }
            result.truncated = collector.truncated;
            result.graph = collector.finish();
        }
        result.row_count = result.row_count.min(u64::from(limit));
        result.page = Some(PageInfo {
            offset: page.offset,
            limit,
            has_more,
        });
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes_identifiers() {
        assert_eq!(ident("a`b"), "`a``b`");
    }
}
