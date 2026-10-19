use serde::{Deserialize, Serialize};

/// Missing estimates stay unknown; zero would misleadingly imply a free operation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanTree {
    pub op: String,
    pub target: Option<String>,
    pub est_rows: Option<f64>,
    pub actual_rows: Option<f64>,
    pub cost: Option<f64>,
    pub children: Vec<PlanTree>,
    pub warnings: Vec<String>,
}
impl PlanTree {
    pub fn warn_estimate(&mut self) {
        if let (Some(estimate), Some(actual)) = (self.est_rows, self.actual_rows) {
            if estimate != actual
                && (estimate == 0.0
                    || actual == 0.0
                    || estimate / actual >= 10.0
                    || actual / estimate >= 10.0)
            {
                self.warnings
                    .push("Estimated and actual rows differ by at least 10x".into());
            }
        }
    }
    pub fn postgres(value: &serde_json::Value) -> Self {
        let mut plan = Self {
            op: value["Node Type"].as_str().unwrap_or("Unknown").into(),
            target: value["Relation Name"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| value["Index Name"].as_str().map(str::to_owned)),
            est_rows: value["Plan Rows"].as_f64(),
            actual_rows: value["Actual Rows"].as_f64(),
            cost: value["Total Cost"].as_f64(),
            children: value["Plans"]
                .as_array()
                .map(|a| a.iter().map(Self::postgres).collect())
                .unwrap_or_default(),
            warnings: vec![],
        };
        if let Some(index) = value["Index Name"].as_str() {
            plan.warnings.push(format!("Index: {index}"));
        }
        plan.warn_estimate();
        plan
    }
    pub fn mongo(value: &serde_json::Value) -> Self {
        let mut children = Vec::new();
        for key in [
            "inputStage",
            "outerStage",
            "innerStage",
            "queryPlan",
            "winningPlan",
            "executionStages",
        ] {
            if value[key].is_object() {
                children.push(Self::mongo(&value[key]));
            }
        }
        for key in ["inputStages", "shards"] {
            if let Some(items) = value[key].as_array() {
                children.extend(items.iter().map(Self::mongo));
            }
        }
        Self {
            op: value["stage"].as_str().unwrap_or("QueryPlan").into(),
            target: value["indexName"]
                .as_str()
                .or_else(|| value["namespace"].as_str())
                .map(str::to_owned),
            est_rows: None,
            actual_rows: value["nReturned"].as_f64(),
            cost: value["executionTimeMillisEstimate"].as_f64(),
            children,
            warnings: vec![],
        }
    }
    pub fn mongo_explain(value: &serde_json::Value, analyze: bool) -> Self {
        let mut plan = if let Some(stages) = value["stages"].as_array() {
            Self {
                op: "Aggregation".into(),
                children: stages
                    .iter()
                    .map(|stage| {
                        if stage["$cursor"].is_object() {
                            Self::mongo_explain(&stage["$cursor"], analyze)
                        } else {
                            Self {
                                op: stage
                                    .as_object()
                                    .and_then(|s| s.keys().find(|k| k.starts_with('$')))
                                    .cloned()
                                    .unwrap_or_else(|| "Stage".into()),
                                actual_rows: stage["nReturned"].as_f64(),
                                cost: stage["executionTimeMillisEstimate"].as_f64(),
                                ..Default::default()
                            }
                        }
                    })
                    .collect(),
                ..Default::default()
            }
        } else {
            Self::mongo(
                if analyze && value["executionStats"]["executionStages"].is_object() {
                    &value["executionStats"]["executionStages"]
                } else {
                    &value["queryPlanner"]["winningPlan"]
                },
            )
        };
        plan.target = value["queryPlanner"]["namespace"]
            .as_str()
            .map(str::to_owned)
            .or(plan.target);
        plan
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_capabilities_default_editor_extensions_off() {
        let mut encoded = serde_json::to_value(crate::Capabilities::default()).unwrap();
        encoded.as_object_mut().unwrap().remove("explain");
        encoded.as_object_mut().unwrap().remove("lint");
        let decoded: crate::Capabilities = serde_json::from_value(encoded).unwrap();
        assert!(!decoded.explain && !decoded.lint);
    }
    #[test]
    fn estimates_and_nested_plans() {
        let p = PlanTree::postgres(
            &serde_json::json!({"Node Type":"Seq Scan","Plan Rows":1,"Actual Rows":10,"Plans":[{"Node Type":"Result"}]}),
        );
        assert_eq!(p.warnings.len(), 1);
        assert_eq!(p.children[0].op, "Result");
        assert_eq!(p.children[0].cost, None);
        let m = PlanTree::mongo(
            &serde_json::json!({"stage":"FETCH","inputStage":{"stage":"IXSCAN","indexName":"pk"}}),
        );
        assert_eq!(m.children[0].target.as_deref(), Some("pk"));
    }
    #[test]
    fn mongo_aggregation_preserves_cursor_and_pipeline_stages() {
        let raw = serde_json::json!({"stages":[
            {"$cursor":{"queryPlanner":{"namespace":"test.people","winningPlan":{"stage":"COLLSCAN"}},"executionStats":{"executionStages":{"stage":"COLLSCAN","nReturned":12}}}},
            {"$group":{"_id":"$team"},"nReturned":3,"executionTimeMillisEstimate":2}
        ]});
        let estimated = PlanTree::mongo_explain(&raw, false);
        assert_eq!(estimated.children[0].op, "COLLSCAN");
        assert_eq!(estimated.children[0].actual_rows, None);
        assert_eq!(estimated.children[0].target.as_deref(), Some("test.people"));
        let actual = PlanTree::mongo_explain(&raw, true);
        assert_eq!(actual.children[0].actual_rows, Some(12.0));
        assert_eq!(actual.children[1].op, "$group");
        assert_eq!(actual.children[1].actual_rows, Some(3.0));
    }
}
