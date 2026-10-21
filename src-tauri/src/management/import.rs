use super::*;
use calamine::{Reader, Xlsx};
use serde_json::value::RawValue;
type JsonRow = std::collections::BTreeMap<String, Box<RawValue>>;
use std::io::{BufRead, BufReader, Cursor};

fn push(data: &mut Vec<Vec<Option<String>>>, row: Vec<Option<String>>) -> Result<(), DbError> {
    if data.len() >= IMPORT_MAX_ROWS {
        return Err(DbError::query("Import exceeds 10000 rows"));
    }
    if row.len() > 512 {
        return Err(DbError::query("Import exceeds 512 columns"));
    }
    data.push(row);
    Ok(())
}
pub fn parse(
    filename: &str,
    bytes: &[u8],
    node_id: String,
    mapping: Option<Vec<ImportMapping>>,
) -> Result<ImportPlan, DbError> {
    if bytes.len() as u64 > IMPORT_MAX_BYTES {
        return Err(DbError::query("Import file exceeds 8 MiB"));
    }
    let format = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut data = vec![];
    let mut source_rows = vec![];
    let mut errors = vec![];
    let mut columns = vec![];

    match format.as_str() {
        "csv" | "tsv" => {
            let mut reader = csv::ReaderBuilder::new()
                .delimiter(if format == "tsv" { b'\t' } else { b',' })
                .flexible(true)
                .from_reader(bytes);
            columns = reader
                .headers()
                .map_err(|_| DbError::query("Invalid CSV header"))?
                .iter()
                .map(str::to_owned)
                .collect();
            for (index, row) in reader.records().enumerate() {
                if index >= IMPORT_MAX_ROWS {
                    return Err(DbError::query("Import exceeds 10000 rows"));
                }
                match row {
                    Ok(r) => {
                        let number = r.position().map(|p| p.line()).unwrap_or(index as u64 + 2);
                        push(
                            &mut data,
                            r.iter()
                                .map(|s| {
                                    if s.is_empty() {
                                        None
                                    } else {
                                        Some(s.to_owned())
                                    }
                                })
                                .collect(),
                        )?;
                        source_rows.push(number);
                    }
                    Err(_) => {
                        if errors.len() < 100 {
                            errors.push(ImportError {
                                row: index as u64 + 2,
                                column: None,
                                message: "Invalid delimited row".into(),
                            });
                        }
                    }
                }
            }
        }
        "json" | "ndjson" | "jsonl" => {
            let mut accept = |object: JsonRow| -> Result<(), DbError> {
                for name in object.keys() {
                    if !columns.contains(name) {
                        if columns.len() >= 512 {
                            return Err(DbError::query("Import exceeds 512 columns"));
                        }
                        columns.push(name.clone());
                        for row in &mut data {
                            row.push(None);
                        }
                    }
                }
                push(
                    &mut data,
                    columns
                        .iter()
                        .map(|name| match object.get(name).map(|v| v.get()) {
                            None | Some("null") => None,
                            Some(raw) if raw.starts_with('"') => {
                                serde_json::from_str::<String>(raw).ok()
                            }
                            Some(raw) => Some(raw.to_owned()),
                        })
                        .collect(),
                )
            };
            if format == "json" {
                struct Rows<'a>(&'a mut dyn FnMut(JsonRow) -> Result<(), DbError>);
                impl<'de> serde::de::Visitor<'de> for Rows<'_> {
                    type Value = ();
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("an array of objects")
                    }
                    fn visit_seq<A: serde::de::SeqAccess<'de>>(
                        self,
                        mut seq: A,
                    ) -> Result<(), A::Error> {
                        while let Some(row) = seq.next_element::<JsonRow>()? {
                            (self.0)(row).map_err(serde::de::Error::custom)?;
                        }
                        Ok(())
                    }
                }
                let mut decoder = serde_json::Deserializer::from_reader(bytes);
                serde::de::Deserializer::deserialize_seq(&mut decoder, Rows(&mut accept))
                    .map_err(|_| DbError::query("Invalid JSON array or import limit exceeded"))?;
                decoder
                    .end()
                    .map_err(|_| DbError::query("Trailing JSON content"))?;
            } else {
                let mut count = 0;
                for (index, line) in BufReader::new(bytes).lines().enumerate() {
                    let line = line.map_err(|_| DbError::query("Invalid NDJSON encoding"))?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    count += 1;
                    if count > IMPORT_MAX_ROWS {
                        return Err(DbError::query("Import exceeds 10000 rows"));
                    }
                    match serde_json::from_str::<JsonRow>(&line) {
                        Ok(v) => {
                            accept(v)?;
                            source_rows.push(index as u64 + 1);
                        }
                        Err(_) => {
                            if errors.len() < 100 {
                                errors.push(ImportError {
                                    row: index as u64 + 1,
                                    column: None,
                                    message: "Invalid JSON object".into(),
                                });
                            }
                        }
                    }
                }
            }
        }
        "xlsx" => {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
                .map_err(|_| DbError::query("Invalid XLSX archive"))?;
            if archive.len() > 2048 {
                return Err(DbError::query("XLSX has too many entries"));
            }
            let mut total = 0u64;
            for i in 0..archive.len() {
                let entry = archive
                    .by_index(i)
                    .map_err(|_| DbError::query("Invalid XLSX entry"))?;
                total = total.saturating_add(entry.size());
                if total > 32 * 1024 * 1024 {
                    return Err(DbError::query("Expanded XLSX exceeds 32 MiB"));
                }
            }
            let mut book: Xlsx<_> = Xlsx::new(Cursor::new(bytes))
                .map_err(|_| DbError::query("Invalid XLSX workbook"))?;
            let sheet = book
                .sheet_names()
                .first()
                .cloned()
                .ok_or_else(|| DbError::query("XLSX has no worksheets"))?;
            let mut cells = book
                .worksheet_cells_reader(&sheet)
                .map_err(|_| DbError::query("Invalid worksheet"))?;
            let mut current = 0u32;
            let mut row: Vec<Option<String>> = vec![];
            let mut last_col = None;
            let mut flush = |row: &mut Vec<Option<String>>,
                             data: &mut Vec<Vec<Option<String>>>,
                             row_number: u64|
             -> Result<(), DbError> {
                if columns.is_empty() {
                    columns = std::mem::take(row)
                        .into_iter()
                        .map(|v| v.unwrap_or_default())
                        .collect();
                    Ok(())
                } else {
                    row.resize(row.len().max(columns.len()), None);
                    push(data, std::mem::take(row))?;
                    source_rows.push(row_number);
                    Ok(())
                }
            };
            while let Some(cell) = cells
                .next_cell()
                .map_err(|_| DbError::query("Invalid worksheet cell"))?
            {
                let (r, c) = cell.get_position();
                if r > IMPORT_MAX_ROWS as u32 || c >= 512 {
                    return Err(DbError::query("Worksheet dimensions exceed import limits"));
                }
                if r < current || (r == current && last_col.is_some_and(|last| c <= last)) {
                    return Err(DbError::query("Worksheet cells are not ordered"));
                }
                if r != current {
                    flush(&mut row, &mut data, current as u64 + 1)?;
                    current = r;
                }
                row.resize(c as usize + 1, None);
                row[c as usize] = match cell.get_value() {
                    calamine::DataRef::Empty => None,
                    calamine::DataRef::Error(_) => {
                        return Err(DbError::query("Worksheet contains an error cell"))
                    }
                    v => Some(calamine::Data::from(v.clone()).to_string()),
                };
                last_col = Some(c);
            }
            if !row.is_empty() {
                flush(&mut row, &mut data, current as u64 + 1)?;
            }
        }
        _ => {
            return Err(DbError::unsupported(
                "Choose CSV, TSV, JSON, NDJSON or XLSX",
            ))
        }
    }
    if columns.is_empty()
        || columns.len() > 512
        || columns.iter().any(|c| c.trim().is_empty())
        || columns
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != columns.len()
    {
        return Err(DbError::query(
            "Headers must be nonempty, unique, and at most 512 columns",
        ));
    }
    let mapping = mapping.unwrap_or_else(|| {
        columns
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let values = data
                    .iter()
                    .filter_map(|r| r.get(i).and_then(Option::as_ref))
                    .collect::<Vec<_>>();
                let ty = if !values.is_empty()
                    && values
                        .iter()
                        .all(|v| v.parse::<i64>().is_ok_and(|n| n.to_string() == v.as_str()))
                {
                    "integer"
                } else if !values.is_empty()
                    && values
                        .iter()
                        .all(|v| matches!(v.as_str(), "true" | "false"))
                {
                    "boolean"
                } else if !values.is_empty()
                    && values.iter().all(|v| {
                        v.parse::<f64>()
                            .is_ok_and(|n| n.is_finite() && n.to_string() == v.as_str())
                    })
                {
                    "float"
                } else {
                    "text"
                };
                ImportMapping {
                    source: name.clone(),
                    target: name.clone(),
                    type_name: ty.into(),
                }
            })
            .collect()
    });
    if mapping.is_empty()
        || mapping.iter().any(|m| {
            !columns.contains(&m.source)
                || m.target.is_empty()
                || !["text", "integer", "float", "boolean", "json"].contains(&m.type_name.as_str())
        })
        || mapping
            .iter()
            .map(|m| &m.target)
            .collect::<std::collections::HashSet<_>>()
            .len()
            != mapping.len()
    {
        return Err(DbError::query("Invalid or duplicate column mapping"));
    }
    let mut edits = vec![];
    let mut sample = vec![];
    for (index, row) in data.iter().enumerate() {
        if row.len() != columns.len() {
            if errors.len() < 100 {
                errors.push(ImportError {
                    row: source_rows.get(index).copied().unwrap_or(index as u64 + 1),
                    column: None,
                    message: "Column count does not match header".into(),
                });
            }
            continue;
        }
        let mut values = vec![];
        let mut valid = true;
        for m in &mapping {
            let Some(column_index) = columns.iter().position(|c| c == &m.source) else { return Err(DbError::query("Invalid or duplicate column mapping")); };
            let raw = &row[column_index];
            let value = if let Some(raw) = raw {
                match m.type_name.as_str() {
                    "text" => Ok(Value::Text(raw.clone())),
                    "integer" => raw
                        .parse::<i64>()
                        .map(|n| Value::Int(n.to_string()))
                        .map_err(|_| ()),
                    "float" => raw
                        .parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .map(|n| Value::Float(FloatValue::Number(n)))
                        .ok_or(()),
                    "boolean" => raw.parse::<bool>().map(Value::Bool).map_err(|_| ()),
                    "json" => serde_json::from_str::<Box<RawValue>>(raw)
                        .map(|_| Value::Json(raw.clone()))
                        .map_err(|_| ()),
                    _ => {
                        return Err(DbError::unsupported(
                            "Import types: text, integer, float, boolean, json",
                        ))
                    }
                }
            } else {
                Ok(Value::Null)
            };
            match value {
                Ok(v) => values.push((m.target.clone(), v)),
                Err(()) => {
                    valid = false;
                    if errors.len() < 100 {
                        errors.push(ImportError {
                            row: source_rows.get(index).copied().unwrap_or(index as u64 + 1),
                            column: Some(m.source.clone()),
                            message: format!("Cannot convert to {}", m.type_name),
                        });
                    }
                }
            }
        }
        if valid {
            if sample.len() < 100 {
                sample.push(values.iter().map(|(_, v)| v.clone()).collect());
            }
            edits.push(RowEdit::Insert { values });
        }
    }
    if !errors.is_empty() {
        edits.clear();
    }
    Ok(ImportPlan{format,columns:mapping,sample,row_count:data.len() as u64,errors,warnings:vec!["Preview checks constraints by executing all valid rows and rolling back; sequences and external trigger effects may persist".into()],request:EditRequest{originals: vec![],create_table: None,node_id,edits,timeout_ms:30000,preview_sample_limit:100}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_empty_strings_and_large_integers_are_preserved() {
        let p=parse("x.json",br#"[{"a":"","b":"001","n":9223372036854775807},{"a":null,"b":"002","n":9223372036854775807}]"#,"x".into(),None).unwrap();
        assert_eq!(p.sample[0][0], Value::Text("".into()));
        assert_eq!(p.sample[1][0], Value::Null);
        assert_eq!(p.sample[0][1], Value::Text("001".into()));
        assert_eq!(p.sample[0][2], Value::Int("9223372036854775807".into()));
    }
    #[test]
    fn json_numbers_and_nested_json_keep_source_precision() {
        let plan = parse("a.json", br#"[{"n":18446744073709551616123456789,"nested":{"n":18446744073709551616123456789}}]"#, "x".into(), None).unwrap();
        assert!(plan.sample[0].contains(&Value::Text("18446744073709551616123456789".into())));
        let plan = parse(
            "a.json",
            br#"[{"nested":{"n":18446744073709551616123456789}}]"#,
            "x".into(),
            Some(vec![ImportMapping {
                source: "nested".into(),
                target: "nested".into(),
                type_name: "json".into(),
            }]),
        )
        .unwrap();
        assert_eq!(
            plan.sample[0][0],
            Value::Json("{\"n\":18446744073709551616123456789}".into())
        );
    }
    #[test]
    fn quoted_csv_and_mapping_errors() {
        let p = parse(
            "a.csv",
            b"id,name\n1,\"a,b\"\n2,x\n",
            "table:t".into(),
            None,
        )
        .unwrap();
        assert_eq!(p.row_count, 2);
        assert_eq!(p.columns[0].type_name, "integer");
        assert_eq!(p.sample[0][1], Value::Text("a,b".into()));
        let m = vec![ImportMapping {
            source: "name".into(),
            target: "n".into(),
            type_name: "integer".into(),
        }];
        let p = parse("a.csv", b"name\nx\n", "table:t".into(), Some(m)).unwrap();
        assert_eq!(p.errors[0].row, 2);
        assert!(p.request.edits.is_empty());
    }
    #[test]
    fn conversion_errors_keep_original_source_line_numbers() {
        let mapping = Some(vec![ImportMapping {
            source: "id".into(),
            target: "id".into(),
            type_name: "integer".into(),
        }]);
        let plan = parse(
            "input.ndjson",
            b"\n{bad}\n{\"id\":\"invalid\"}\n",
            "x".into(),
            mapping,
        )
        .unwrap();
        assert_eq!(
            plan.errors.iter().map(|e| e.row).collect::<Vec<_>>(),
            [2, 3]
        );
        assert!(plan.request.edits.is_empty());
    }
    #[test]
    fn json_union_samples_and_row_cap() {
        let p = parse("a.ndjson", b"{\"a\":1}\n{\"b\":true}\n", "x".into(), None).unwrap();
        assert_eq!(p.columns.len(), 2);
        assert_eq!(p.sample[0][1], Value::Null);
        let many = format!("a\n{}", "1\n".repeat(101));
        assert_eq!(
            parse("a.tsv", many.as_bytes(), "x".into(), None)
                .unwrap()
                .sample
                .len(),
            100
        );
        assert!(parse("a.csv", b"a,a\n1,2\n", "x".into(), None).is_err());
    }
}
#[cfg(test)]
mod xlsx_tests {
    use super::*;
    use std::io::Write;
    fn workbook(cell: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(vec![]));
        for (name,content) in [
            ("_rels/.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".into()),
            ("[Content_Types].xml","<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/></Types>".into()),
            ("xl/workbook.xml","<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>".into()),
            ("xl/_rels/workbook.xml.rels","<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>".into()),
            ("xl/worksheets/sheet1.xml",format!("<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>id</t></is></c></row><row r=\"2\"><c r=\"{cell}\"><v>7</v></c></row></sheetData></worksheet>"))
        ]{zip.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(content.as_bytes()).unwrap();}
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn xlsx_streaming_and_sparse_dimension_limit() {
        let p = parse("a.xlsx", &workbook("A2"), "x".into(), None).unwrap();
        assert_eq!(p.sample[0][0], Value::Int("7".into()));
        assert!(parse("a.xlsx", &workbook("XFD1048576"), "x".into(), None).is_err());
    }
}
