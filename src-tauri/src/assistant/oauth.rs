//! Official OSS SIWC flow: /siwc/token-sharing-open-source/sign-in (no borrowed client IDs).
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
#[derive(Clone, Serialize, Deserialize)]
struct Account {
    client_id: String,
    subject: String,
    label: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    id_token: String,
    #[serde(default)]
    scopes: String,
    #[serde(default)]
    expires_at: u64,
}
impl Account {
    fn clear_tokens(&mut self) {
        self.access_token.clear();
        self.refresh_token.clear();
        self.id_token.clear();
        self.expires_at = 0;
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Vault {
    active: Option<String>,
    accounts: BTreeMap<String, Account>,
}
fn lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    &LOCK
}
async fn process_lock(config: &std::path::Path) -> Result<std::fs::File, DbError> {
    // shortcut: a shared OAuth vault requires one config root; use a global session lock before supporting multiple roots.
    let path = config.join("ai-openai.lock");
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|_| DbError::internal("Cannot open AI session lock"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(35);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(_) => return Err(DbError::query("ChatGPT session is busy; retry shortly")),
        }
    }
}
fn entry() -> Result<keyring::Entry, DbError> {
    keyring::Entry::new("axdb", "ai:openai:oauth")
        .map_err(|_| DbError::internal("AI keychain unavailable"))
}
fn load() -> Result<Vault, DbError> {
    match entry()?.get_password() {
        Ok(s) => serde_json::from_str(&s)
            .map_err(|_| DbError::internal("Invalid OpenAI keychain record")),
        Err(keyring::Error::NoEntry) => Ok(Vault::default()),
        Err(_) => Err(DbError::internal("Cannot read AI keychain")),
    }
}
fn save(vault: &Vault) -> Result<(), DbError> {
    entry()?
        .set_password(
            &serde_json::to_string(vault)
                .map_err(|_| DbError::internal("Cannot encode AI credentials"))?,
        )
        .map_err(|_| DbError::internal("Cannot save AI credentials"))
}
pub(super) fn accounts() -> Result<Value, DbError> {
    let vault = load()?;
    Ok(json!(vault.accounts.iter().map(|(id,a)|json!({"id":id,"label":format!("{} · {id}",a.label),"active":vault.active.as_ref()==Some(id),"logged_in":!a.access_token.is_empty(),"expired":a.expires_at<=now()})).collect::<Vec<_>>()))
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(super) fn client() -> Result<reqwest::Client, DbError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| DbError::internal("Cannot initialize AI HTTP client"))
}
pub(super) async fn json_response(mut response: reqwest::Response) -> Result<Value, DbError> {
    if !response.status().is_success() {
        return Err(DbError::query(format!(
            "AI request failed (HTTP {})",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| DbError::query("AI response interrupted"))?
    {
        if bytes.len() + chunk.len() > 2_000_000 {
            return Err(DbError::query("AI response exceeds limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| DbError::query("Invalid AI response"))
}
fn checked_token(value: &Value, field: &str) -> Result<String, DbError> {
    let s = value[field]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 32000 && !s.chars().any(char::is_whitespace))
        .ok_or_else(|| DbError::query("Invalid OAuth token response"))?;
    Ok(s.into())
}
fn token_update(account: &mut Account, value: &Value) -> Result<(), DbError> {
    let access = checked_token(value, "access_token")?;
    if value["token_type"]
        .as_str()
        .is_none_or(|v| !v.eq_ignore_ascii_case("bearer"))
    {
        return Err(DbError::query("Invalid OAuth token type"));
    }
    let scopes = value["scope"]
        .as_str()
        .unwrap_or(&account.scopes)
        .to_owned();
    if !["chatgpt.tokens.use.direct", "resource.invoke"]
        .iter()
        .all(|s| scopes.split_whitespace().any(|v| v == *s))
    {
        return Err(DbError::query("ChatGPT plan permission was not granted"));
    }
    let expires = value["expires_in"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 86400)
        .ok_or_else(|| DbError::query("Invalid OAuth expiry"))?;
    let refresh = if value.get("refresh_token").is_some() {
        checked_token(value, "refresh_token")?
    } else {
        account.refresh_token.clone()
    };
    if refresh.is_empty() {
        return Err(DbError::query("OAuth refresh token missing"));
    }
    account.access_token = access;
    account.refresh_token = refresh;
    account.scopes = scopes;
    account.expires_at = now().saturating_add(expires);
    Ok(())
}
pub(super) async fn access_token(config: &std::path::Path) -> Result<String, DbError> {
    let _guard = lock().lock().await;
    let _process = process_lock(config).await?;
    let mut vault = load()?;
    let id = vault
        .active
        .clone()
        .ok_or_else(|| DbError::query("Continue with ChatGPT first"))?;
    let account = vault
        .accounts
        .get_mut(&id)
        .ok_or_else(|| DbError::query("ChatGPT account not found"))?;
    if account.access_token.is_empty() {
        return Err(DbError::query("Continue with ChatGPT first"));
    }
    if account.expires_at <= now() + 60 {
        let response = client()?
            .post(TOKEN)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &account.client_id),
                ("refresh_token", &account.refresh_token),
                ("resource", RESOURCE),
            ])
            .send()
            .await
            .map_err(|_| DbError::query("ChatGPT refresh unavailable; retry shortly"))?;
        if matches!(response.status().as_u16(), 400 | 401) {
            account.clear_tokens();
            save(&vault)?;
            return Err(DbError::query(
                "ChatGPT session expired or revoked; sign in again",
            ));
        }
        let value = json_response(response).await?;
        token_update(account, &value)?;
        let access = account.access_token.clone();
        save(&vault)?;
        return Ok(access);
    }
    Ok(account.access_token.clone())
}
struct Pending {
    state: String,
    nonce: String,
    verifier: String,
    redirect: String,
    client_id: String,
    previous: Option<Account>,
}
fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn auth_url(pending: &Pending, host_id: &str) -> String {
    // INVARIANT: AUTHORIZE is a compile-time constant containing the valid OpenAI authorization URL.
    #[allow(clippy::expect_used)]
    let mut url = reqwest::Url::parse(AUTHORIZE).expect("valid authorization URL");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pending.verifier.as_bytes()));
    {
        let mut query = url.query_pairs_mut();
        query.extend_pairs([
            ("client_id", pending.client_id.as_str()),
            ("ext_agent_host_id", host_id),
            ("response_type", "code"),
            ("redirect_uri", &pending.redirect),
            ("scope", SCOPES),
            ("resource", RESOURCE),
            ("state", &pending.state),
            ("nonce", &pending.nonce),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge),
        ]);
        if pending.client_id == "dynamic_agent_client" {
            query.append_pair("agent_name_hint", "AxDB");
        }
        // No ID-token hint crosses the webview boundary; returning sign-in uses account selection.
    }
    url.into()
}
fn callback(pending: &Pending, target: &str) -> Result<(String, String), DbError> {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| DbError::query("Invalid OAuth callback"))?;
    if url.path() != "/auth/callback" {
        return Err(DbError::query("Invalid OAuth callback path"));
    }
    let mut pairs = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if pairs.insert(key.to_string(), value.to_string()).is_some() {
            return Err(DbError::query("Duplicate OAuth callback parameter"));
        }
    }
    if pairs.get("state") != Some(&pending.state) {
        return Err(DbError::query("OAuth state mismatch"));
    }
    if pairs.contains_key("error") {
        return Err(DbError::query("ChatGPT authorization declined"));
    }
    let code = pairs
        .get("code")
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or_else(|| DbError::query("OAuth code missing"))?
        .clone();
    let id = pairs
        .get("client_id")
        .cloned()
        .unwrap_or_else(|| pending.client_id.clone());
    if id == "dynamic_agent_client"
        || id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || id.len() > 512
        || (pending.client_id != "dynamic_agent_client" && id != pending.client_id)
    {
        return Err(DbError::query("OAuth client registration mismatch"));
    }
    Ok((code, id))
}
fn decode_part(s: &str) -> Result<Value, DbError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| DbError::query("Invalid OAuth ID token"))?;
    serde_json::from_slice(&bytes).map_err(|_| DbError::query("Invalid OAuth ID token"))
}
fn verify_id(
    token: &str,
    jwks: &Value,
    client_id: &str,
    nonce: &str,
    previous: Option<&Account>,
) -> Result<(String, String), DbError> {
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || token.len() > 32000 {
        return Err(DbError::query("Invalid OAuth ID token"));
    }
    let header = decode_part(parts[0])?;
    let kid = header["kid"]
        .as_str()
        .ok_or_else(|| DbError::query("OAuth ID token key missing"))?;
    if header["alg"] != "RS256" {
        return Err(DbError::query("OAuth ID token algorithm rejected"));
    }
    let jwk = jwks["keys"]
        .as_array()
        .and_then(|keys| {
            keys.iter().find(|k| {
                k["kid"] == kid
                    && k["kty"] == "RSA"
                    && k.get("use").is_none_or(|v| v == "sig")
                    && k.get("alg").is_none_or(|v| v == "RS256")
            })
        })
        .ok_or_else(|| DbError::query("OAuth signing key unavailable"))?;
    let decode = |field: &str| {
        URL_SAFE_NO_PAD
            .decode(jwk[field].as_str().unwrap_or(""))
            .map_err(|_| DbError::query("Invalid OAuth signing key"))
    };
    let n = decode("n")?;
    let e = decode("e")?;
    let signature = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| DbError::query("Invalid OAuth ID token signature"))?;
    ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &signature,
        )
        .map_err(|_| DbError::query("OAuth ID token signature rejected"))?;
    let claims = decode_part(parts[1])?;
    let audience = claims["aud"] == client_id
        || claims["aud"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == client_id));
    let sub = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DbError::query("OAuth identity missing"))?;
    if claims["iss"] != "https://auth.openai.com"
        || !audience
        || claims["exp"].as_u64().is_none_or(|exp| exp <= now())
        || claims["nonce"] != nonce
        || claims["aud"]
            .as_array()
            .is_some_and(|a| a.len() > 1 && claims["azp"] != client_id)
        || claims["iat"].as_u64().is_some_and(|n| n > now() + 30)
        || claims["nbf"].as_u64().is_some_and(|n| n > now() + 30)
        || previous.is_some_and(|a| a.subject != sub)
    {
        return Err(DbError::query("OAuth identity validation failed"));
    }
    Ok((sub.into(), claims["email"].as_str().unwrap_or(sub).into()))
}
struct Attempt {
    stop: tokio_util::sync::CancellationToken,
    result: std::sync::Arc<std::sync::Mutex<Value>>,
}
fn slot() -> &'static tokio::sync::Mutex<Option<Attempt>> {
    static SLOT: tokio::sync::Mutex<Option<Attempt>> = tokio::sync::Mutex::const_new(None);
    &SLOT
}
pub(super) async fn start(host: &Host, account_id: Option<String>) -> Result<Value, DbError> {
    cancel().await;
    let _guard = lock().lock().await;
    let _process = process_lock(&host.paths.config).await?;
    let host_path = host.paths.config.join("ai-openai-host.json");
    let host_id = match std::fs::read(&host_path) {
        Ok(bytes) => serde_json::from_slice::<String>(&bytes)
            .map_err(|_| DbError::internal("Invalid AI host ID"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
            storage::write_private(
                &host_path,
                &serde_json::to_vec(&id)
                    .map_err(|_| DbError::internal("Cannot encode AI host ID"))?,
            )?;
            id
        }
        Err(_) => return Err(DbError::internal("Cannot read AI host ID")),
    };
    let vault = load()?;
    let selected = if account_id.as_deref() == Some("") {
        None
    } else {
        account_id.or(vault.active.clone())
    };
    let previous = selected
        .map(|id| {
            vault
                .accounts
                .get(&id)
                .cloned()
                .ok_or_else(|| DbError::query("Unknown ChatGPT account"))
        })
        .transpose()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| DbError::query("Cannot start OAuth loopback listener"))?;
    let addr = listener
        .local_addr()
        .map_err(|_| DbError::internal("Cannot read callback address"))?;
    let pending = Pending {
        state: random(),
        nonce: random(),
        verifier: random(),
        redirect: format!("http://127.0.0.1:{}/auth/callback", addr.port()),
        client_id: previous
            .as_ref()
            .map(|a| a.client_id.clone())
            .unwrap_or_else(|| "dynamic_agent_client".into()),
        previous,
    };
    let url = auth_url(&pending, &host_id);
    let stop = tokio_util::sync::CancellationToken::new();
    let result = std::sync::Arc::new(std::sync::Mutex::new(json!({"state":"waiting"})));
    *slot().lock().await = Some(Attempt {
        stop: stop.clone(),
        result: result.clone(),
    });
    let config = host.paths.config.clone();
    tokio::spawn(async move {
        let outcome = tokio::select! { _=stop.cancelled()=>return, result=tokio::time::timeout(Duration::from_secs(600),finish_login(listener,pending,&stop,&config))=>result };
        let value = match outcome {
            Ok(Ok(label)) => json!({"state":"done","account":label}),
            _ => json!({"state":"failed"}),
        };
        if let Ok(mut result) = result.lock() {
            *result = value;
        }
    });
    Ok(json!({"url":url,"waiting_for_code":false}))
}
async fn finish_login(
    listener: tokio::net::TcpListener,
    pending: Pending,
    stop: &tokio_util::sync::CancellationToken,
    config: &std::path::Path,
) -> Result<String, DbError> {
    // Ignore unrelated loopback requests without consuming this authorization attempt.
    let (code, client_id) = loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|_| DbError::query("OAuth listener closed"))?;
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        let read = tokio::time::timeout(Duration::from_secs(2), async {
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buf).await?;
                if n == 0 || request.len() + n > 16384 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            Ok::<(), std::io::Error>(())
        })
        .await;
        if !matches!(read, Ok(Ok(()))) {
            continue;
        }
        let line = std::str::from_utf8(&request)
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");
        let pieces = line.split_whitespace().collect::<Vec<_>>();
        let outcome = if pieces.len() == 3 && pieces[0] == "GET" {
            callback(&pending, pieces[1])
        } else {
            Err(DbError::query("Invalid OAuth callback"))
        };
        let status = if outcome.is_ok() {
            "200 OK"
        } else {
            "400 Bad Request"
        };
        let response=format!("HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nReturn to AxDB to check sign-in status.");
        let _ = socket.write_all(response.as_bytes()).await;
        if let Ok(value) = outcome {
            break value;
        }
        // An explicit denial with the right state settles immediately; unrelated requests do not.
        if pieces.get(1).is_some_and(|s| {
            s.contains(&format!("state={}", pending.state)) && s.contains("error=")
        }) {
            return Err(DbError::query("ChatGPT authorization declined"));
        }
    };
    let client = client()?;
    let value = json_response(
        client
            .post(TOKEN)
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client_id),
                ("code", &code),
                ("code_verifier", &pending.verifier),
                ("redirect_uri", &pending.redirect),
                ("resource", RESOURCE),
            ])
            .send()
            .await
            .map_err(|_| DbError::query("OAuth exchange failed"))?,
    )
    .await?;
    let id_token = checked_token(&value, "id_token")?;
    let jwks = json_response(
        client
            .get("https://auth.openai.com/.well-known/jwks.json")
            .send()
            .await
            .map_err(|_| DbError::query("OAuth signing keys unavailable"))?,
    )
    .await?;
    let (subject, label) = verify_id(
        &id_token,
        &jwks,
        &client_id,
        &pending.nonce,
        pending.previous.as_ref(),
    )?;
    let mut account = Account {
        client_id: client_id.clone(),
        subject,
        label: label.clone(),
        access_token: String::new(),
        refresh_token: String::new(),
        id_token,
        scopes: String::new(),
        expires_at: 0,
    };
    token_update(&mut account, &value)?;
    let _guard = lock().lock().await;
    let _process = process_lock(config).await?;
    if stop.is_cancelled() {
        return Err(DbError::query("OAuth cancelled"));
    }
    let mut vault = load()?;
    if vault
        .accounts
        .get(&client_id)
        .is_some_and(|a| a.subject != account.subject)
    {
        return Err(DbError::query("OAuth account registration mismatch"));
    }
    vault.active = Some(client_id.clone());
    vault.accounts.insert(client_id, account);
    save(&vault)?;
    Ok(label)
}
pub(super) async fn poll() -> Value {
    let mut slot = slot().lock().await;
    let Some(attempt) = slot.as_ref() else {
        return json!({"state":"idle"});
    };
    let value = attempt
        .result
        .lock()
        .map(|v| v.clone())
        .unwrap_or_else(|_| json!({"state":"failed"}));
    if value["state"] != "waiting" {
        *slot = None;
    }
    value
}
pub(super) async fn cancel() {
    if let Some(attempt) = slot().lock().await.take() {
        attempt.stop.cancel();
    }
}
fn revocation_endpoint(metadata: &Value) -> Result<reqwest::Url, DbError> {
    let url = metadata["revocation_endpoint"]
        .as_str()
        .and_then(|s| reqwest::Url::parse(s).ok())
        .filter(|u| {
            u.scheme() == "https"
                && u.host_str() == Some("auth.openai.com")
                && u.port_or_known_default() == Some(443)
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
        })
        .ok_or_else(|| DbError::query("OpenAI revocation endpoint unavailable"))?;
    Ok(url)
}
async fn revoke(account: &Account) -> Result<bool, DbError> {
    let client = client()?;
    let metadata = json_response(
        client
            .get("https://auth.openai.com/.well-known/openid-configuration")
            .send()
            .await
            .map_err(|_| DbError::query("OpenAI discovery unavailable"))?,
    )
    .await?;
    let endpoint = revocation_endpoint(&metadata)?;
    for attempt in 0..3 {
        let response = client
            .post(endpoint.clone())
            .form(&[
                ("token", account.refresh_token.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", account.client_id.as_str()),
            ])
            .send()
            .await;
        match response {
            Ok(r) if r.status().as_u16() == 200 => return Ok(true),
            Ok(r) if !r.status().is_server_error() => return Ok(false),
            _ if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(200 << attempt)).await;
            }
            _ => {}
        }
    }
    Ok(false)
}
pub(super) async fn logout_account(config: &std::path::Path, account_id: Option<String>) -> Result<bool, DbError> {
    let _guard = lock().lock().await;
    let _process = process_lock(config).await?;
    let mut vault = load()?;
    let Some(id) = account_id.or_else(||vault.active.clone()) else {
        return Ok(true);
    };
    let account = vault
        .accounts
        .get_mut(&id)
        .ok_or_else(|| DbError::query("Unknown ChatGPT account"))?;
    let revoked = if account.refresh_token.is_empty() {
        true
    } else {
        revoke(account).await.unwrap_or(false)
    };
    account.clear_tokens();
    save(&vault)?;
    Ok(revoked)
}
#[tauri::command]
pub async fn ai_account_select(
    host: tauri::State<'_, Host>,
    account_id: String,
) -> Result<(), DbError> {
    let _guard = lock().lock().await;
    let _process = process_lock(&host.paths.config).await?;
    let mut vault = load()?;
    if !vault.accounts.contains_key(&account_id) {
        return Err(DbError::query("Unknown ChatGPT account"));
    }
    vault.active = Some(account_id);
    save(&vault)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_cannot_send_refresh_tokens_outside_openai() {
        assert!(revocation_endpoint(
            &json!({"revocation_endpoint":"https://auth.openai.com/oauth/revoke"})
        )
        .is_ok());
        for url in [
            "http://auth.openai.com/revoke",
            "https://auth.openai.com.attacker.invalid/revoke",
            "https://attacker.invalid/revoke",
            "https://user:password@auth.openai.com/revoke",
            "https://auth.openai.com:8443/revoke",
            "https://auth.openai.com/revoke?token=anything",
            "https://auth.openai.com/revoke#anything",
        ] {
            assert!(revocation_endpoint(&json!({"revocation_endpoint":url})).is_err());
        }
        assert!(revocation_endpoint(&json!({})).is_err());
    }
    fn pending() -> Pending {
        Pending {
            state: "state".into(),
            nonce: "nonce".into(),
            verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
            redirect: "http://127.0.0.1:51234/auth/callback".into(),
            client_id: "dynamic_agent_client".into(),
            previous: None,
        }
    }
    #[tokio::test]
    async fn session_file_lock_serializes_independent_handles() {
        let dir = tempfile::tempdir().unwrap();
        let first = process_lock(dir.path()).await.unwrap();
        let second = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("ai-openai.lock"))
            .unwrap();
        assert!(matches!(
            second.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(first);
        assert!(second.try_lock().is_ok());
    }
    #[test]
    fn pkce_and_callback_bind_state_path_and_issued_client() {
        let p = pending();
        let url = reqwest::Url::parse(&auth_url(&p, "urn:uuid:test")).unwrap();
        let pairs = url.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(
            pairs["code_challenge"],
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(pairs["client_id"], "dynamic_agent_client");
        assert_eq!(pairs["resource"], RESOURCE);
        assert!(callback(
            &p,
            "/auth/callback?code=a&state=state&client_id=oaiapp_test"
        )
        .is_ok());
        for target in [
            "/callback?code=a&state=state&client_id=oaiapp_test",
            "/auth/callback?code=a&state=wrong&client_id=oaiapp_test",
            "/auth/callback?code=a&state=state",
            "/auth/callback?error=access_denied&state=state",
            "/auth/callback?code=a&state=state&state=state&client_id=oaiapp_test",
        ] {
            assert!(callback(&p, target).is_err());
        }
        let mut p = p;
        p.client_id = "oaiapp_previous".into();
        assert!(callback(&p, "/auth/callback?code=a&state=state").is_ok());
        assert!(callback(
            &p,
            "/auth/callback?code=a&state=state&client_id=oaiapp_other"
        )
        .is_err());
    }
    #[test]
    fn signed_id_token_checks_signature_identity_issuer_audience_expiry_and_nonce() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ai-id-token.json")).unwrap();
        let verify = |name: &str, previous: Option<&Account>| {
            verify_id(
                fixture[name].as_str().unwrap(),
                &fixture["jwks"],
                "oaiapp_test",
                "nonce",
                previous,
            )
        };
        assert_eq!(verify("valid", None).unwrap().0, "test-subject");
        for name in ["issuer", "audience", "expired", "nonce"] {
            assert!(verify(name, None).is_err(), "{name}");
        }
        let account = Account {
            client_id: "oaiapp_test".into(),
            subject: "test-subject".into(),
            label: "test".into(),
            access_token: String::new(),
            refresh_token: String::new(),
            id_token: String::new(),
            scopes: String::new(),
            expires_at: 0,
        };
        assert!(verify("subject", Some(&account)).is_err());
        let mut token = fixture["valid"].as_str().unwrap().to_string();
        let end = token.len() - 4;
        token.replace_range(end..end + 1, "!");
        assert!(verify_id(&token, &fixture["jwks"], "oaiapp_test", "nonce", None).is_err());
    }
    #[test]
    fn refresh_rotates_atomically_and_rejects_missing_scope() {
        let mut a = Account {
            client_id: "oaiapp_test".into(),
            subject: "s".into(),
            label: "test".into(),
            access_token: "old".into(),
            refresh_token: "old_refresh".into(),
            id_token: "id".into(),
            scopes: SCOPES.into(),
            expires_at: 0,
        };
        token_update(&mut a,&json!({"access_token":"new","refresh_token":"rotated","token_type":"Bearer","expires_in":3600})).unwrap();
        assert_eq!(a.refresh_token, "rotated");
        assert!(a.expires_at > now());
        assert!(token_update(
            &mut a,
            &json!({"access_token":"bad","token_type":"Bearer","expires_in":3600,"scope":"openid"})
        )
        .is_err());
        assert_eq!(a.access_token, "new");
        a.clear_tokens();
        assert_eq!(a.client_id, "oaiapp_test");
        assert_eq!(a.subject, "s");
        assert!(a.access_token.is_empty() && a.refresh_token.is_empty() && a.id_token.is_empty());
        assert_eq!(a.expires_at, 0);
        let invalid = format!(
            "{}.{}.{}",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"none","kid":"test"}"#),
            URL_SAFE_NO_PAD.encode(br#"{"sub":"s"}"#),
            "bad"
        );
        assert!(verify_id(&invalid, &json!({"keys":[]}), "oaiapp_test", "nonce", None).is_err());
    }
}
