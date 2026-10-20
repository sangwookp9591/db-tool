use super::model::*;
use dbcore::DbError;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct Parsed {
    pub driver_id: String,
    pub fields: BTreeMap<String, String>,
    pub network: Option<Network>,
    pub has_secret: bool,
    pub sanitized_input: String,
    #[serde(skip)]
    pub secret: Option<String>,
}
fn decode(value: &str) -> Result<String, DbError> {
    percent_decode_str(value)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| error("INVALID_CONNECTION_STRING"))
}
pub fn parse(input: &str) -> Result<Parsed, DbError> {
    let input = input.trim();
    if input.is_empty() || input.len() > 65536 {
        return Err(error("INVALID_CONNECTION_STRING"));
    }
    if input
        .lines()
        .next()
        .is_some_and(|l| l.trim_start().to_ascii_lowercase().starts_with("host "))
    {
        return ssh_config(input);
    }
    if input.starts_with("PGPASSWORD=") {
        let mut tokens = shell_words::split(input).map_err(|_| error("INVALID_PSQL_COMMAND"))?;
        let password = tokens
            .remove(0)
            .strip_prefix("PGPASSWORD=")
            .unwrap_or_default()
            .to_owned();
        if tokens.first().map(String::as_str) != Some("psql") {
            return Err(error("INVALID_PSQL_COMMAND"));
        }
        let mut parsed = psql(&shell_words::join(tokens))?;
        parsed.secret = Some(password);
        return Ok(finish(parsed));
    }
    if input.starts_with("psql ") || input.starts_with("psql\t") {
        return psql(input);
    }
    let text = input.strip_prefix("jdbc:").unwrap_or(input);
    if text.contains("://") {
        return uri(text);
    }
    if input.contains("rds.amazonaws.com") && !input.contains(char::is_whitespace) {
        return uri(&format!("postgresql://{input}"));
    }
    if input.contains('=') {
        return keyword(input);
    }
    Err(error("UNRECOGNIZED_CONNECTION_STRING"))
}
fn empty(driver: &str) -> Parsed {
    Parsed {
        driver_id: driver.into(),
        fields: BTreeMap::new(),
        network: None,
        has_secret: false,
        sanitized_input: String::new(),
        secret: None,
    }
}
fn finish(mut p: Parsed) -> Parsed {
    p.has_secret = p.secret.is_some();
    // Reconstruct from non-secret fields, never return any fragment of the pasted password.
    p.sanitized_input = p
        .fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ");
    p
}
fn uri(text: &str) -> Result<Parsed, DbError> {
    let mut u = url::Url::parse(text).map_err(|_| error("INVALID_CONNECTION_STRING"))?;
    let driver = match u.scheme() {
        "postgres" | "postgresql" => "postgres",
        "mongodb" => "mongodb",
        "redis" | "rediss" => "redis",
        "bolt" | "neo4j" | "bolt+s" | "bolt+ssc" | "neo4j+s" | "neo4j+ssc" => "neo4j",
        _ => return Err(error("UNSUPPORTED_CONNECTION_SCHEME")),
    };
    let mut p = empty(driver);
    let host = u
        .host_str()
        .ok_or_else(|| error("INVALID_HOST"))?
        .trim_matches(['[', ']'])
        .to_owned();
    valid_host(&host)?;
    let port = u.port().unwrap_or(match driver {
        "postgres" => 5432,
        "mongodb" => 27017,
        "redis" => 6379,
        _ => 7687,
    });
    if port == 0 {
        return Err(error("INVALID_PORT"));
    }
    if !u.username().is_empty() {
        p.fields.insert(
            if driver == "redis" {
                "username"
            } else {
                "user"
            }
            .into(),
            decode(u.username())?,
        );
    }
    p.secret = u.password().map(decode).transpose()?;
    let db = decode(u.path().trim_start_matches('/'))?;
    if !db.is_empty() {
        p.fields
            .insert(if driver == "redis" { "db" } else { "database" }.into(), db);
    }
    let mut safe_query = Vec::new();
    for (key, value) in u.query_pairs() {
        match key.as_ref() {
            "password" | "sslpassword" => p.secret = Some(value.into_owned()),
            "user" | "username" => {
                p.fields.insert(
                    if driver == "redis" {
                        "username"
                    } else {
                        "user"
                    }
                    .into(),
                    value.into_owned(),
                );
            }
            "sslmode" | "sslrootcert" | "tls" | "tlsCAFile" | "authSource" => {
                p.fields
                    .insert(key.clone().into_owned(), value.clone().into_owned());
                safe_query.push((key.into_owned(), value.into_owned()));
            }
            "replicaSet" | "directConnection" if driver == "mongodb" => {
                safe_query.push((key.into_owned(), value.into_owned()));
            }
            _ => return Err(error("UNSUPPORTED_CONNECTION_OPTION")),
        }
    }
    u.set_username("")
        .map_err(|_| error("INVALID_CONNECTION_STRING"))?;
    u.set_password(None)
        .map_err(|_| error("INVALID_CONNECTION_STRING"))?;
    u.set_query(None);
    if !safe_query.is_empty() {
        u.query_pairs_mut().extend_pairs(safe_query);
    }
    u.set_fragment(None);
    if driver == "mongodb" || driver == "neo4j" {
        p.fields.insert("uri".into(), u.to_string());
    } else {
        p.fields.insert("host".into(), host);
        p.fields.insert("port".into(), port.to_string());
    }
    if text.starts_with("rediss:") {
        return Err(error("REDIS_TLS_UNSUPPORTED"));
    }
    Ok(finish(p))
}
fn keyword(input: &str) -> Result<Parsed, DbError> {
    let mut p = empty("postgres");
    for token in shell_words::split(input).map_err(|_| error("INVALID_CONNECTION_STRING"))? {
        let (key, value) = token
            .split_once('=')
            .ok_or_else(|| error("INVALID_CONNECTION_STRING"))?;
        match key {
            "password" => p.secret = Some(value.into()),
            "host" | "port" | "user" | "database" | "sslmode" | "sslrootcert" => {
                p.fields.insert(key.into(), value.into());
            }
            "dbname" => {
                p.fields.insert("database".into(), value.into());
            }
            _ => return Err(error("UNSUPPORTED_CONNECTION_OPTION")),
        }
    }
    if let Some(host) = p.fields.get("host") {
        valid_host(host)?;
    }
    if let Some(port) = p.fields.get("port") {
        port.parse::<u16>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| error("INVALID_PORT"))?;
    }
    Ok(finish(p))
}
fn psql(input: &str) -> Result<Parsed, DbError> {
    let tokens = shell_words::split(input).map_err(|_| error("INVALID_PSQL_COMMAND"))?;
    let mut p = empty("postgres");
    let mut iter = tokens.iter().skip(1);
    while let Some(token) = iter.next() {
        if token.starts_with("--")
            && let Some((flag, value)) = token.split_once('=')
        {
            let key = match flag {
                "--host" => "host",
                "--port" => "port",
                "--username" => "user",
                "--dbname" => "database",
                _ => return Err(error("UNSUPPORTED_PSQL_OPTION")),
            };
            if key == "database" && (value.contains("://") || value.contains('=')) {
                let parsed = parse(value)?;
                if parsed.driver_id != "postgres" {
                    return Err(error("INVALID_PSQL_COMMAND"));
                }
                p.fields.extend(parsed.fields);
                p.secret = parsed.secret;
            } else {
                p.fields.insert(key.into(), value.into());
            }
            continue;
        }
        let key = match token.as_str() {
            "-h" | "--host" => "host",
            "-p" | "--port" => "port",
            "-U" | "--username" => "user",
            "-d" | "--dbname" => "database",
            "-W" | "-w" => continue,
            _ => {
                if token.contains("://") || token.contains('=') {
                    let parsed = parse(token)?;
                    if parsed.driver_id != "postgres" {
                        return Err(error("INVALID_PSQL_COMMAND"));
                    }
                    p.fields.extend(parsed.fields);
                    p.secret = parsed.secret;
                    continue;
                }
                if token.starts_with('-') {
                    return Err(error("UNSUPPORTED_PSQL_OPTION"));
                }
                if p.fields.contains_key("database") {
                    return Err(error("INVALID_PSQL_COMMAND"));
                }
                p.fields.insert("database".into(), token.clone());
                continue;
            }
        };
        let value = iter.next().ok_or_else(|| error("INVALID_PSQL_COMMAND"))?;
        if key == "database" && (value.contains("://") || value.contains('=')) {
            let parsed = parse(value)?;
            if parsed.driver_id != "postgres" {
                return Err(error("INVALID_PSQL_COMMAND"));
            }
            p.fields.extend(parsed.fields);
            p.secret = parsed.secret;
        } else {
            p.fields.insert(key.into(), value.clone());
        }
    }
    if let Some(host) = p.fields.get("host") {
        valid_host(host)?;
    }
    if let Some(port) = p.fields.get("port") {
        port.parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| error("INVALID_PORT"))?;
    }
    Ok(finish(p))
}
fn ssh_config(input: &str) -> Result<Parsed, DbError> {
    let mut values = BTreeMap::new();
    for line in input.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line
            .split_once([' ', '\t', '='])
            .ok_or_else(|| error("INVALID_SSH_CONFIG"))?;
        let k = k.to_ascii_lowercase();
        if values
            .insert(k, v.trim().trim_matches('"').to_owned())
            .is_some()
        {
            return Err(error("MULTIPLE_SSH_HOSTS"));
        }
    }
    let alias = values
        .get("host")
        .ok_or_else(|| error("INVALID_SSH_CONFIG"))?;
    if alias.contains(['*', '?', '!', ' ']) {
        return Err(error("SSH_HOST_PATTERN_UNSUPPORTED"));
    }
    let host = values.get("hostname").unwrap_or(alias).clone();
    let port = values
        .get("port")
        .map(|s| s.parse::<u16>().map_err(|_| error("INVALID_PORT")))
        .transpose()?
        .unwrap_or(22);
    let user = values.get("user").cloned().unwrap_or_default();
    let key_file = values.get("identityfile").cloned();
    let mut jumps = Vec::new();
    if let Some(jump) = values.get("proxyjump") {
        for value in jump.split(',') {
            let (user, host) = value
                .split_once('@')
                .ok_or_else(|| error("SSH_JUMP_USER_REQUIRED"))?;
            let (host, port) = host
                .rsplit_once(':')
                .map(|(h, p)| Ok((h, p.parse::<u16>().map_err(|_| error("INVALID_PORT"))?)))
                .transpose()?
                .unwrap_or((host, 22));
            jumps.push(SshHop {
                host: host.into(),
                port,
                user: user.into(),
                auth: SshAuth::Agent,
                key_file: None,
            });
        }
    }
    if values.contains_key("proxycommand") {
        return Err(error("SSH_PROXYCOMMAND_UNSUPPORTED"));
    }
    let mut p = empty("postgres");
    p.network = Some(Network {
        ssh: Some(Ssh {
            host,
            port,
            user,
            auth: if key_file.is_some() {
                SshAuth::Key
            } else {
                SshAuth::Agent
            },
            key_file,
            jump_hosts: jumps,
        }),
        proxy: None,
    });
    Ok(finish(p))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_supported_inputs_and_removes_secrets() {
        let p = parse("jdbc:postgresql://db.test:5433/app?user=alice&password=secret").unwrap();
        assert_eq!(p.fields["user"], "alice");
        assert!(!p.fields.values().any(|v| v.contains("secret")));
        let p = parse("postgres://alice:p%40ss@db.test:5433/app?sslmode=verify-full").unwrap();
        assert_eq!(p.secret.as_deref(), Some("p@ss"));
        assert_eq!(p.fields["database"], "app");
        assert!(!serde_json::to_string(&p).unwrap().contains("p@ss"));
        let p =
            parse("psql -h db.test -p 5433 -U alice -d 'host=db.test dbname=app password=secret'")
                .unwrap();
        assert_eq!(p.fields["database"], "app");
        assert!(p.has_secret);
        assert!(!p.sanitized_input.contains("secret"));
        assert_eq!(
            parse("db.abc.us-east-1.rds.amazonaws.com:5432")
                .unwrap()
                .fields["host"],
            "db.abc.us-east-1.rds.amazonaws.com"
        );
        let p = parse("mongodb://alice:secret@db.test:27017/app?authSource=admin").unwrap();
        assert!(!p.fields["uri"].contains("secret"));
        assert_eq!(p.fields["user"], "alice");
        let p =
            parse("mongodb://alice:secret@db.test:27017/app?replicaSet=team&directConnection=true")
                .unwrap();
        assert!(p.fields["uri"].contains("replicaSet=team"));
        assert!(!p.fields["uri"].contains("secret"));
        let p = parse("redis://:secret@db.test:6379/2").unwrap();
        assert_eq!(p.fields["db"], "2");
    }
    #[test]
    fn parses_ssh_config_without_executing_commands() {
        let p=parse("Host prod\n HostName bastion.test\n User alice\n IdentityFile ~/.ssh/test\n ProxyJump bob@jump.test:2222,carol@jump2.test").unwrap();
        let ssh = p.network.unwrap().ssh.unwrap();
        assert_eq!(ssh.jump_hosts.len(), 2);
        assert!(matches!(ssh.auth, SshAuth::Key));
        assert!(parse("Host *\n ProxyCommand malicious").is_err());
        assert!(parse("psql -c 'DROP DATABASE x'").is_err());
    }
    #[test]
    fn rejects_invalid_ports_and_non_postgres_psql_targets() {
        for input in [
            "postgresql://db.test:0/app",
            "psql -p invalid app",
            "psql -p 0 app",
            "psql 'redis://db.test/0'",
        ] {
            assert!(parse(input).is_err(), "accepted invalid input: {input}");
        }
    }
}
