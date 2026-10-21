use super::model::{Auth, error};
use base64::Engine;
use dbcore::DbError;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use time::OffsetDateTime;

pub struct Token {
    pub value: String,
    pub expires: Instant,
}
impl Token {
    pub fn fresh(&self) -> bool {
        self.expires.saturating_duration_since(Instant::now()) > Duration::from_secs(120)
    }
}
struct Credentials {
    access: String,
    secret: String,
    session: Option<String>,
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|v| format!("{v:02x}")).collect()
}
fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    // INVARIANT: HMAC accepts keys of every length, including an empty key.
    #[allow(clippy::expect_used)]
    let mut m = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(data.as_bytes());
    m.finalize().into_bytes().to_vec()
}
fn encode(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn signature(
    secret: &str,
    date: &str,
    region: &str,
    service: &str,
    string_to_sign: &str,
) -> String {
    let d = hmac(format!("AWS4{secret}").as_bytes(), date);
    let r = hmac(&d, region);
    let s = hmac(&r, service);
    let signing = hmac(&s, "aws4_request");
    hex(hmac(&signing, string_to_sign))
}
struct Signing<'a> {
    host: &'a str,
    path: &'a str,
    region: &'a str,
    service: &'a str,
    date: &'a str,
    expires: u32,
    payload: &'a str,
}
fn presign(c: &Credentials, s: Signing<'_>, mut query: Vec<(String, String)>) -> String {
    let scope = format!("{}/{}/{}/aws4_request", &s.date[..8], s.region, s.service);
    query.extend([
        ("X-Amz-Algorithm".into(), "AWS4-HMAC-SHA256".into()),
        ("X-Amz-Credential".into(), format!("{}/{}", c.access, scope)),
        ("X-Amz-Date".into(), s.date.into()),
        ("X-Amz-Expires".into(), s.expires.to_string()),
        ("X-Amz-SignedHeaders".into(), "host".into()),
    ]);
    if let Some(token) = &c.session {
        query.push(("X-Amz-Security-Token".into(), token.clone()));
    }
    let mut encoded: Vec<_> = query
        .into_iter()
        .map(|(k, v)| (encode(&k), encode(&v)))
        .collect();
    encoded.sort();
    let query = encoded
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical = format!(
        "GET\n{}\n{query}\nhost:{}\n\nhost\n{}",
        s.path, s.host, s.payload
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        s.date,
        hex(Sha256::digest(canonical.as_bytes()))
    );
    format!(
        "{}{path}?{query}&X-Amz-Signature={sig}",
        s.host,
        path = s.path,
        sig = signature(
            &c.secret,
            &s.date[..8],
            s.region,
            s.service,
            &string_to_sign
        )
    )
}
pub fn home() -> Result<PathBuf, DbError> {
    std::env::home_dir().ok_or_else(|| error("HOME_UNAVAILABLE"))
}
fn ini(path: &Path) -> Result<BTreeMap<String, BTreeMap<String, String>>, DbError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(error("IAM_PROFILE_UNREADABLE")),
    };
    let mut sections = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].into();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            sections
                .entry(section.clone())
                .or_default()
                .insert(k.trim().into(), v.trim().into());
        }
    }
    Ok(sections)
}
#[derive(Serialize)]
pub struct Profiles {
    pub aws: Vec<String>,
    pub gcp: Vec<String>,
}
pub fn profiles() -> Result<Profiles, DbError> {
    let root = home()?;
    let mut names = std::collections::BTreeSet::new();
    for n in ini(&root.join(".aws/credentials"))?.into_keys() {
        names.insert(n);
    }
    for n in ini(&root.join(".aws/config"))?.into_keys() {
        if n == "default" {
            names.insert(n);
        } else if let Some(n) = n.strip_prefix("profile ") {
            names.insert(n.into());
        }
    }
    let adc = adc_path(None)?;
    Ok(Profiles {
        aws: names.into_iter().collect(),
        gcp: if adc.is_file() {
            vec![adc.to_string_lossy().into_owned()]
        } else {
            vec![]
        },
    })
}
fn aws_profile(root: &Path, name: &str) -> Result<BTreeMap<String, String>, DbError> {
    let mut config = ini(&root.join(".aws/config"))?
        .remove(&if name == "default" {
            name.to_owned()
        } else {
            format!("profile {name}")
        })
        .unwrap_or_default();
    if let Some(values) = ini(&root.join(".aws/credentials"))?.remove(name) {
        config.extend(values);
    }
    Ok(config)
}
fn http_client() -> Result<reqwest::Client, DbError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| error("IAM_HTTP_INIT"))
}
async fn aws_credentials(name: &str) -> Result<Credentials, DbError> {
    let root = home()?;
    let config = aws_profile(&root, name)?;
    if let (Some(access), Some(secret)) = (
        config.get("aws_access_key_id"),
        config.get("aws_secret_access_key"),
    ) {
        return Ok(Credentials {
            access: access.clone(),
            secret: secret.clone(),
            session: config.get("aws_session_token").cloned(),
        });
    }
    let session = config.get("sso_session");
    let session_config = if let Some(session) = session {
        ini(&root.join(".aws/config"))?
            .remove(&format!("sso-session {session}"))
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    let get = |key: &str| {
        config
            .get(key)
            .or_else(|| session_config.get(key))
            .cloned()
            .ok_or_else(|| error("AWS_SSO_PROFILE_INCOMPLETE"))
    };
    let start = get("sso_start_url")?;
    let region = get("sso_region")?;
    valid_region(&region)?;
    let cache_key = session.unwrap_or(&start);
    let cache = hex(sha1::Sha1::digest(cache_key.as_bytes()));
    let cached = read_json(&root.join(format!(".aws/sso/cache/{cache}.json")))?;
    let expiry = cached
        .get("expiresAt")
        .and_then(|v| v.as_str())
        .and_then(|s| OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok())
        .ok_or_else(|| error("AWS_SSO_LOGIN_REQUIRED"))?;
    if expiry.unix_timestamp() <= OffsetDateTime::now_utc().unix_timestamp() + 120 {
        return Err(error("AWS_SSO_LOGIN_REQUIRED"));
    }
    let token = string(&cached, "accessToken")?;
    let response = http_client()?
        .get(format!(
            "https://portal.sso.{region}.amazonaws.com/federation/credentials"
        ))
        .header("x-amz-sso_bearer_token", token)
        .query(&[
            ("account_id", get("sso_account_id")?),
            ("role_name", get("sso_role_name")?),
        ])
        .send()
        .await
        .map_err(|_| error("AWS_SSO_REQUEST_FAILED"))?;
    if !response.status().is_success() {
        return Err(error("AWS_SSO_LOGIN_REQUIRED"));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| error("AWS_SSO_INVALID_RESPONSE"))?;
    let value = value
        .get("roleCredentials")
        .ok_or_else(|| error("AWS_SSO_INVALID_RESPONSE"))?;
    Ok(Credentials {
        access: string(value, "accessKeyId")?,
        secret: string(value, "secretAccessKey")?,
        session: Some(string(value, "sessionToken")?),
    })
}
fn string(value: &serde_json::Value, key: &str) -> Result<String, DbError> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| error("IAM_CREDENTIAL_INVALID"))
}
fn read_json(path: &Path) -> Result<serde_json::Value, DbError> {
    let bytes = std::fs::read(path).map_err(|_| error("IAM_CREDENTIAL_UNREADABLE"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(error("IAM_CREDENTIAL_TOO_LARGE"));
    }
    serde_json::from_slice(&bytes).map_err(|_| error("IAM_CREDENTIAL_INVALID"))
}
fn valid_region(region: &str) -> Result<(), DbError> {
    if region.len() > 64
        || region.is_empty()
        || !region
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        Err(error("INVALID_AWS_REGION"))
    } else {
        Ok(())
    }
}
fn aws_region(root: &Path, profile: &str, explicit: &str, host: &str) -> Result<String, DbError> {
    let region = if !explicit.is_empty() {
        explicit.to_owned()
    } else {
        aws_profile(root, profile)?
            .remove("region")
            .filter(|r| !r.is_empty())
            .or_else(|| {
                host.split_once(".rds.")
                    .and_then(|(prefix, _)| prefix.rsplit('.').next())
                    .map(str::to_owned)
            })
            .ok_or_else(|| error("AWS_REGION_REQUIRED"))?
    };
    valid_region(&region)?;
    Ok(region)
}
fn adc_path(path: Option<&str>) -> Result<PathBuf, DbError> {
    Ok(if let Some(path) = path {
        PathBuf::from(path)
    } else if let Some(path) = std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS") {
        path.into()
    } else {
        {
            #[cfg(windows)]
            {
                PathBuf::from(std::env::var_os("APPDATA").ok_or_else(|| error("HOME_UNAVAILABLE"))?)
                    .join("gcloud/application_default_credentials.json")
            }
            #[cfg(not(windows))]
            {
                home()?.join(".config/gcloud/application_default_credentials.json")
            }
        }
    })
}
async fn gcp_token(path: Option<&str>) -> Result<Token, DbError> {
    let adc = read_json(&adc_path(path)?)?;
    let client = http_client()?;
    let form = match string(&adc, "type")?.as_str() {
        "authorized_user" => vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("client_id".into(), string(&adc, "client_id")?),
            ("client_secret".into(), string(&adc, "client_secret")?),
            ("refresh_token".into(), string(&adc, "refresh_token")?),
        ],
        "service_account" => {
            let now = OffsetDateTime::now_utc().unix_timestamp();
            let claims = serde_json::json!({"iss":string(&adc,"client_email")?,"scope":"https://www.googleapis.com/auth/sqlservice.admin","aud":"https://oauth2.googleapis.com/token","iat":now,"exp":now+3600});
            let pem = string(&adc, "private_key")?;
            let der = base64::engine::general_purpose::STANDARD
                .decode(
                    pem.lines()
                        .filter(|l| !l.starts_with("---"))
                        .collect::<String>(),
                )
                .map_err(|_| error("GCP_ADC_KEY_INVALID"))?;
            let key = ring::signature::RsaKeyPair::from_pkcs8(&der)
                .map_err(|_| error("GCP_ADC_KEY_INVALID"))?;
            let enc = &base64::engine::general_purpose::URL_SAFE_NO_PAD;
            let header = enc.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
            let body =
                enc.encode(serde_json::to_vec(&claims).map_err(|_| error("GCP_ADC_KEY_INVALID"))?);
            let signing = format!("{header}.{body}");
            let mut sig = vec![0; key.public().modulus_len()];
            key.sign(
                &ring::signature::RSA_PKCS1_SHA256,
                &ring::rand::SystemRandom::new(),
                signing.as_bytes(),
                &mut sig,
            )
            .map_err(|_| error("GCP_ADC_KEY_INVALID"))?;
            let jwt = format!("{signing}.{}", enc.encode(sig));
            vec![
                (
                    "grant_type".into(),
                    "urn:ietf:params:oauth:grant-type:jwt-bearer".into(),
                ),
                ("assertion".into(), jwt),
            ]
        }
        _ => return Err(error("GCP_ADC_TYPE_UNSUPPORTED")),
    };
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form)
        .finish();
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| error("GCP_TOKEN_REQUEST_FAILED"))?;
    if !response.status().is_success() {
        return Err(error("GCP_ADC_LOGIN_REQUIRED"));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| error("GCP_TOKEN_INVALID_RESPONSE"))?;
    let expires = value
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .filter(|n| *n > 120 && *n <= 86400)
        .ok_or_else(|| error("GCP_TOKEN_INVALID_RESPONSE"))?;
    Ok(Token {
        value: string(&value, "access_token")?,
        expires: Instant::now() + Duration::from_secs(expires),
    })
}
pub async fn token(auth: &Auth, host: &str, port: u16, user: &str) -> Result<Token, DbError> {
    match auth {
        Auth::Password => Err(error("IAM_NOT_CONFIGURED")),
        Auth::IamAws { profile, region } => {
            let region = aws_region(&home()?, profile, region, host)?;
            let credentials = aws_credentials(profile).await?;
            let format = time::format_description::parse_borrowed::<2>(
                "[year][month][day]T[hour][minute][second]Z",
            )
            .map_err(|_| error("IAM_CLOCK_ERROR"))?;
            let date = OffsetDateTime::now_utc()
                .format(&format)
                .map_err(|_| error("IAM_CLOCK_ERROR"))?;
            let value = presign(
                &credentials,
                Signing {
                    host: &format!("{host}:{port}"),
                    path: "/",
                    region: &region,
                    service: "rds-db",
                    date: &date,
                    expires: 900,
                    payload: &hex(Sha256::digest([])),
                },
                vec![
                    ("Action".into(), "connect".into()),
                    ("DBUser".into(), user.into()),
                ],
            );
            Ok(Token {
                value,
                expires: Instant::now() + Duration::from_secs(900),
            })
        }
        Auth::IamGcp { adc_file } => gcp_token(adc_file.as_deref()).await,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_aws_presigned_s3_vector() {
        // AWS publishes these example credentials and expected signature; they are not usable secrets.
        let c = Credentials {
            access: "AKIAIOSFODNN7EXAMPLE".into(),
            secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            session: None,
        };
        let url = presign(
            &c,
            Signing {
                host: "examplebucket.s3.amazonaws.com",
                path: "/test.txt",
                region: "us-east-1",
                service: "s3",
                date: "20130524T000000Z",
                expires: 86400,
                payload: "UNSIGNED-PAYLOAD",
            },
            vec![],
        );
        assert!(url.ends_with(
            "X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        ));
    }
    #[test]
    fn rds_encoding_and_session_token() {
        let c = Credentials {
            access: "test".into(),
            secret: "test".into(),
            session: Some("a+b/c=".into()),
        };
        let u = presign(
            &c,
            Signing {
                host: "db.test:5432",
                path: "/",
                region: "us-east-1",
                service: "rds-db",
                date: "20261019T000000Z",
                expires: 900,
                payload: &hex(Sha256::digest([])),
            },
            vec![
                ("Action".into(), "connect".into()),
                ("DBUser".into(), "user+name".into()),
            ],
        );
        assert!(u.contains("DBUser=user%2Bname"));
        assert!(u.contains("X-Amz-Security-Token=a%2Bb%2Fc%3D"));
        assert!(u.contains("X-Amz-Expires=900"));
    }
    #[test]
    fn expires_before_server_deadline() {
        assert!(
            !Token {
                value: String::new(),
                expires: Instant::now() + Duration::from_secs(119)
            }
            .fresh()
        );
        assert!(
            Token {
                value: String::new(),
                expires: Instant::now() + Duration::from_secs(899)
            }
            .fresh()
        );
    }
    #[test]
    fn region_uses_profile_then_rds_endpoint_with_explicit_override() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".aws")).unwrap();
        std::fs::write(
            root.path().join(".aws/config"),
            "[profile team]\nregion=ap-northeast-2\n",
        )
        .unwrap();
        let endpoint = "db.cluster-id.us-west-2.rds.amazonaws.com";
        assert_eq!(
            aws_region(root.path(), "team", "", endpoint).unwrap(),
            "ap-northeast-2"
        );
        assert_eq!(
            aws_region(root.path(), "missing", "", endpoint).unwrap(),
            "us-west-2"
        );
        assert_eq!(
            aws_region(root.path(), "team", "eu-west-1", endpoint).unwrap(),
            "eu-west-1"
        );
        assert!(aws_region(root.path(), "missing", "", "db.test").is_err());
    }
}
