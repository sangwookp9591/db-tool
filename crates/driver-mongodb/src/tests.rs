use super::*;
use mongodb::bson::{doc, oid::ObjectId, spec::BinarySubtype, Binary, DateTime, Decimal128};
use std::str::FromStr;

#[test]
fn maps_bson_without_precision_or_field_order_loss() {
    assert_eq!(
        map_value(&Bson::Int64(i64::MAX)),
        Value::Int(i64::MAX.to_string())
    );
    let decimal = "123456789012345678901234567890.1234";
    assert_eq!(
        map_value(&Bson::Decimal128(Decimal128::from_str(decimal).unwrap())),
        Value::Decimal(decimal.into())
    );
    let oid = ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap();
    assert_eq!(
        map_value(&Bson::ObjectId(oid)),
        Value::Other {
            type_name: "ObjectId".into(),
            text: oid.to_hex()
        }
    );
    assert_eq!(
        map_value(&Bson::DateTime(DateTime::from_millis(1))),
        Value::DateTimeTz("1970-01-01T00:00:00.001Z".into())
    );
    assert_eq!(
        map_value(&Bson::Binary(Binary {
            subtype: BinarySubtype::Generic,
            bytes: vec![0, 255]
        })),
        Value::Bytes {
            len: 2,
            hex: "00ff".into()
        }
    );
    assert_eq!(
        map_value(&Bson::Document(doc! {"z": [true, Bson::Null], "a": 1})),
        Value::Map(vec![
            (
                "z".into(),
                Value::List(vec![Value::Bool(true), Value::Null])
            ),
            ("a".into(), Value::Int("1".into()))
        ])
    );
    assert_eq!(
        map_value(&Bson::Double(f64::NAN)),
        Value::Float(FloatValue::Special("NaN".into()))
    );
}

#[test]
fn parses_one_extended_json_command_and_rejects_shell_and_duplicates() {
    let command =
        parse_command(r#"{"find":"x","filter":{"n":{"$numberLong":"9223372036854775807"}}}"#)
            .unwrap();
    assert_eq!(command.keys().next().unwrap(), "find");
    assert_eq!(
        command
            .get_document("filter")
            .unwrap()
            .get_i64("n")
            .unwrap(),
        i64::MAX
    );
    for text in [
        "db.x.find()",
        "[]",
        "{}",
        r#"{"find":"x"} {"insert":"x"}"#,
        r#"{"find":"x","find":"y"}"#,
    ] {
        assert!(parse_command(text).is_err(), "{text}");
    }
}

#[test]
fn read_only_fails_closed_for_commands_and_nested_write_stages() {
    for text in [
        r#"{"insert":"x","documents":[]}"#,
        r#"{"aggregate":"x","pipeline":[{"$out":"y"}],"cursor":{}}"#,
        r#"{"aggregate":"x","pipeline":[{"$lookup":{"from":"y","pipeline":[{"$merge":"z"}],"as":"y"}}],"cursor":{}}"#,
        r#"{"aggregate":"x","pipeline":[{"$futureWrite":{}}],"cursor":{}}"#,
        r#"{"explain":{"insert":"x"}}"#,
        r#"{"find":"x","$db":"admin"}"#,
    ] {
        assert!(
            check_read_only(&parse_command(text).unwrap()).is_err(),
            "{text}"
        );
    }
    for text in [
        r#"{"find":"x","filter":{}}"#,
        r#"{"aggregate":"x","pipeline":[{"$match":{}},{"$group":{"_id":"$x","n":{"$sum":1}}}],"cursor":{}}"#,
    ] {
        check_read_only(&parse_command(text).unwrap()).unwrap();
    }
}

#[test]
fn table_unions_sampled_fields_and_pads_missing_values() {
    let result = document_result(vec![doc! {"z":1}, doc! {"a":true}], false);
    assert_eq!(result.documents.as_ref().unwrap().len(), 2);
    let table = result.table.unwrap();
    assert_eq!(
        table
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["z", "a"]
    );
    assert_eq!(table.rows[0], vec![Value::Int("1".into()), Value::Null]);
    assert_eq!(table.rows[1], vec![Value::Null, Value::Bool(true)]);
}
