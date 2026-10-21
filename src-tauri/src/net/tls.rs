use super::{Endpoint, error};
use dbcore::DbError;
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Tls {
    pub connector: tokio_native_tls::TlsConnector,
}
pub fn immediate(
    fields: &BTreeMap<String, String>,
    endpoint: &Endpoint,
) -> Result<Option<Tls>, DbError> {
    let Some(uri) = &endpoint.uri else {
        return Ok(None);
    };
    let scheme = uri.scheme();
    let mongo_tls = if let Some(value) = fields
        .get("tls")
        .filter(|v| matches!(v.as_str(), "true" | "false"))
    {
        value == "true"
    } else {
        uri.query_pairs()
            .any(|(k, v)| (k == "tls" || k == "ssl") && v == "true")
    };
    if !(scheme.ends_with("+s") || scheme.ends_with("+ssc") || scheme == "mongodb" && mongo_tls) {
        return Ok(None);
    }
    let mut connector = native_tls::TlsConnector::builder();
    if scheme.ends_with("+ssc") {
        connector
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true);
    }
    let ca = fields.get("tlsCAFile").cloned().or_else(|| {
        uri.query_pairs()
            .find(|(k, _)| k == "tlsCAFile")
            .map(|(_, v)| v.into_owned())
    });
    if let Some(path) = ca.filter(|v| !v.is_empty()) {
        let pem = std::fs::read(path).map_err(|_| error("TLS_CA_UNREADABLE"))?;
        let certs =
            native_tls::Certificate::stack_from_pem(&pem).map_err(|_| error("TLS_CA_INVALID"))?;
        if certs.is_empty() {
            return Err(error("TLS_CA_INVALID"));
        }
        connector.disable_built_in_roots(true);
        for cert in certs {
            connector.add_root_certificate(cert);
        }
    }
    Ok(Some(Tls {
        connector: connector
            .build()
            .map_err(|_| error("TLS_INIT_FAILED"))?
            .into(),
    }))
}
pub fn local_plaintext(fields: &mut BTreeMap<String, String>) -> Result<(), DbError> {
    let Some(uri) = fields.get("uri") else {
        return Ok(());
    };
    let mut uri = url::Url::parse(uri).map_err(|_| error("INVALID_CONNECTION_URI"))?;
    let scheme = uri
        .scheme()
        .trim_end_matches("+ssc")
        .trim_end_matches("+s")
        .to_owned();
    uri.set_scheme(&scheme)
        .map_err(|_| error("INVALID_CONNECTION_URI"))?;
    let query: Vec<(String, String)> = uri
        .query_pairs()
        .filter(|(k, _)| !k.to_ascii_lowercase().starts_with("tls") && k != "ssl")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    uri.set_query(None);
    uri.query_pairs_mut().extend_pairs(query);
    fields.remove("tlsCAFile");
    if scheme == "mongodb" {
        fields.insert("tls".into(), "false".into());
        uri.query_pairs_mut().append_pair("tls", "false");
    }
    fields.insert("uri".into(), uri.into());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offload_removes_only_transport_tls_options() {
        let mut fields=BTreeMap::from([("uri".into(),"mongodb://127.0.0.1:55555/?tls=true&tlsCAFile=ca&authSource=admin&directConnection=true".into()),("tlsCAFile".into(),"ca".into())]);
        local_plaintext(&mut fields).unwrap();
        assert!(!fields.contains_key("tlsCAFile"));
        assert_eq!(fields["tls"], "false");
        assert!(fields["uri"].contains("authSource=admin"));
        assert!(fields["uri"].contains("tls=false"));
    }
}
