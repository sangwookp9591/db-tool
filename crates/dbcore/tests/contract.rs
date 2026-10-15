use dbcore::*;
use serde_json::json;

#[test]
fn values_preserve_tags_and_precision() {
    assert_eq!(
        serde_json::to_value(Value::Null).unwrap(),
        json!({"t":"Null"})
    );
    assert_eq!(
        serde_json::to_value(Value::Int("9223372036854775807".into())).unwrap(),
        json!({"t":"Int", "v":"9223372036854775807"})
    );
    assert_eq!(
        serde_json::to_value(Value::Bytes {
            len: 2,
            hex: "abcd".into()
        })
        .unwrap(),
        json!({"t":"Bytes", "v":{"len":2,"hex":"abcd"}})
    );
    assert_eq!(
        serde_json::to_value(Value::Float(FloatValue::Special("NaN".into()))).unwrap(),
        json!({"t":"Float","v":"NaN"})
    );
}

#[test]
fn debug_never_exposes_secrets() {
    let cfg = ConnectConfig {
        fields: [
            ("password".into(), "field-sensitive".into()),
            ("host".into(), "localhost".into()),
        ]
        .into(),
        secret: Some("sensitive".into()),
        connect_timeout_ms: 1000,
    };
    let debug = format!("{cfg:?}");
    assert!(!debug.contains("sensitive"));
    assert!(debug.contains("***"));
    assert_eq!(clamp_row_limit(0), 1);
    assert_eq!(clamp_row_limit(u32::MAX), 100_000);
}

#[test]
fn preview_contract_is_additive_and_defaults_to_no_support() {
    let req: ExecRequest = serde_json::from_value(json!({"text":"SELECT 1"})).unwrap();
    assert_eq!(req.preview_sample_limit, 50);
    let caps: Capabilities = serde_json::from_value(json!({"cancel":"None","timeout":false,"params":false,"transactions":false,"paged_browse":false,"graph_expand":false,"multi_statement":false,"edit_rows":false})).unwrap();
    assert_eq!(caps.preview_changes, PreviewSupport::None);
    assert!(
        !serde_json::to_value(ChangePreview::default()).unwrap()["committed"]
            .as_bool()
            .unwrap()
    );
}
