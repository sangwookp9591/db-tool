use super::*;
use async_trait::async_trait;

pub(super) const OPENAI_ENDPOINT: &str = "https://api.openai.com/v1/responses";
pub(super) type Emit<'a> = &'a mut (dyn FnMut(String) -> Result<(), DbError> + Send + 'a);

#[async_trait]
pub(super) trait AiProvider {
    async fn login(&self, host: &Host, account: Option<String>) -> Result<Value, DbError>;
    async fn refresh(&self, host: &Host, settings: &Settings) -> Result<Credential, DbError>;
    async fn complete(
        &self,
        credential: &Credential,
        body: Value,
        emit: Emit<'_>,
    ) -> Result<Value, DbError>;
}
pub(super) struct Provider(pub String);
impl Provider {
    pub fn new(name: &str) -> Result<Self, DbError> {
        if !["anthropic", "openai"].contains(&name) {
            return Err(DbError::query("Unknown AI provider"));
        }
        Ok(Self(name.into()))
    }
}
#[async_trait]
impl AiProvider for Provider {
    async fn login(&self, host: &Host, account: Option<String>) -> Result<Value, DbError> {
        if self.0 == "anthropic" {
            ai_login_start().await
        } else {
            oauth::start(host, account).await
        }
    }
    async fn refresh(&self, host: &Host, settings: &Settings) -> Result<Credential, DbError> {
        credential(settings, &host.paths.config).await
    }
    async fn complete(
        &self,
        credential: &Credential,
        body: Value,
        emit: Emit<'_>,
    ) -> Result<Value, DbError> {
        let endpoint = if self.0 == "openai" {
            OPENAI_ENDPOINT
        } else {
            ENDPOINT
        };
        super::send(endpoint, credential, body, emit).await
    }
}
pub(super) async fn send(
    name: &str,
    credential: &Credential,
    body: Value,
    emit: Emit<'_>,
) -> Result<Value, DbError> {
    Provider::new(name)?.complete(credential, body, emit).await
}
pub(super) fn request_body(
    settings: &Settings,
    context: Value,
    question: &str,
    tools: &[Value],
) -> Value {
    let mut body = super::request_body(settings, context, question);
    if settings.provider == "openai" {
        let instructions = body["system"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        body = json!({"model":settings.model,"instructions":instructions,"input":body["messages"],"store":false,"stream":true,"max_output_tokens":8192,"reasoning":{"effort":settings.effort}});
        if efforts(&settings.provider, &settings.model).is_empty() { body.as_object_mut().unwrap().remove("reasoning"); }
        if !tools.is_empty() {
            body["tools"] = json!(tools.iter().map(|tool|json!({"type":"function","name":tool["name"],"description":tool["description"],"parameters":tool["input_schema"],"strict":false})).collect::<Vec<_>>());
            body["tool_choice"] = json!("required");
            body["parallel_tool_calls"] = json!(false);
        }
    } else if !tools.is_empty() {
        body["tools"] = json!(tools);
        // Opus 5.5 and other thinking-first models reject forced tool choice.
        body["tool_choice"] = json!({"type":"auto","disable_parallel_tool_use":true});
        if let Some(content) = body["messages"][0]["content"].as_str() {
            body["messages"][0]["content"]=json!(format!("Use the propose_operation tool to return the structured plan. Never execute it or return only prose.\n{content}"));
        }
    }
    if settings.provider == "anthropic" && settings.model.starts_with("claude-opus-4-5") {
        body.as_object_mut().unwrap().remove("thinking");
    }
    if settings.provider == "anthropic" && efforts(&settings.provider, &settings.model).is_empty() {
        body.as_object_mut().unwrap().remove("thinking");
        body.as_object_mut().unwrap().remove("output_config");
    }
    body
}
fn usage_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}
pub(super) fn usage(host: &Host) -> Result<Value, DbError> {
    let path = host.paths.config.join("ai-usage.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| DbError::internal("Invalid AI usage"))?;
            if !["anthropic", "openai"].iter().all(|provider| {
                ["input_tokens", "output_tokens"]
                    .iter()
                    .all(|field| value[*provider][*field].is_u64())
            }) {
                return Err(DbError::internal("Invalid AI usage"));
            }
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(
            json!({"anthropic":{"input_tokens":0,"output_tokens":0},"openai":{"input_tokens":0,"output_tokens":0}}),
        ),
        Err(_) => Err(DbError::internal("Cannot read AI usage")),
    }
}
pub(super) fn record_usage(host: &Host, provider: &str, tokens: &Value) -> Result<(), DbError> {
    let _guard = usage_lock()
        .lock()
        .map_err(|_| DbError::internal("AI usage unavailable"))?;
    let mut totals = usage(host)?;
    for field in ["input_tokens", "output_tokens"] {
        totals[provider][field] = json!(totals[provider][field]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(tokens[field].as_u64().unwrap_or(0)));
    }
    storage::write_private(
        &host.paths.config.join("ai-usage.json"),
        &serde_json::to_vec(&totals).map_err(|_| DbError::internal("Cannot encode AI usage"))?,
    )
}

#[tauri::command]
pub async fn ai_provider_login_start(
    host: tauri::State<'_, Host>,
    provider: String,
    account_id: Option<String>,
) -> Result<Value, DbError> {
    Provider::new(&provider)?.login(&host, account_id).await
}
#[tauri::command]
pub async fn ai_provider_login_poll(provider: String) -> Result<Value, DbError> {
    Provider::new(&provider)?;
    if provider == "anthropic" {
        ai_login_poll().await
    } else {
        Ok(oauth::poll().await)
    }
}
#[tauri::command]
pub async fn ai_provider_login_cancel(provider: String) -> Result<(), DbError> {
    Provider::new(&provider)?;
    if provider == "anthropic" {
        ai_login_cancel().await
    } else {
        oauth::cancel().await;
        Ok(())
    }
}
#[tauri::command]
pub async fn ai_provider_logout(
    host: tauri::State<'_, Host>,
    provider: String,
    account_id: Option<String>,
) -> Result<Value, DbError> {
    Provider::new(&provider)?;
    if provider == "openai" {
        oauth::cancel().await;
        let remove_key = account_id.is_none();
        let remote_revoked = oauth::logout_account(&host.paths.config, account_id).await?;
        if remove_key { delete_key(&provider)?; }
        return Ok(json!({"logged_in":false,"remote_revoked":remote_revoked}));
    }
    if provider == "anthropic" && ant_cli().is_some() {
        ai_logout().await?;
    }
    delete_key(&provider)?;
    Ok(json!({"logged_in":false}))
}
// Model capability metadata follows the official effort docs (see W7_REPORT.md).
pub(super) fn efforts(provider: &str, model: &str) -> &'static [&'static str] {
    if provider == "anthropic" {
        if ["claude-opus-5", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-5", "claude-haiku-5-5", "claude-fable-5", "claude-mythos-5"].iter().any(|prefix| model == *prefix || model.starts_with(&format!("{prefix}-"))) {
            &["low", "medium", "high", "xhigh", "max"]
        } else if ["claude-opus-4-6", "claude-sonnet-4-6", "claude-mythos-preview"].iter().any(|prefix| model.starts_with(prefix)) {
            &["low", "medium", "high", "max"]
        } else if model.starts_with("claude-opus-4-5") { &["low", "medium", "high"] } else { &[] }
    } else if ["gpt-6-astra", "gpt-6.1-sol", "gpt-6-sol", "gpt-6-luna"].iter().any(|prefix|model==*prefix || model.strip_prefix(prefix).is_some_and(|suffix|suffix.starts_with("-20"))) {
        &["low", "medium", "high", "xhigh", "max"]
    } else if ["gpt-5.2", "gpt-5.4", "gpt-5.5"].iter().any(|prefix|model.starts_with(prefix)) && !model.contains("pro") {
        &["none", "low", "medium", "high", "xhigh"]
    } else if model.starts_with("gpt-5.1") && !model.contains("pro") { &["none", "low", "medium", "high"]
    } else if ["o3", "o4-mini"].iter().any(|prefix| model==*prefix || model.strip_prefix(prefix).is_some_and(|suffix|suffix.starts_with("-20"))) { &["low", "medium", "high"]
    } else { &[] }
}
fn catalog_models(value: &Value, provider: &str, status: &str, cached: bool, auth: &str) -> Result<Value, DbError> {
    let models = value.as_array().or_else(|| value["data"].as_array()).or_else(||value["models"].as_array())
        .ok_or_else(|| DbError::query("Invalid AI model catalog"))?;
    Ok(json!(models.iter().filter_map(|m| {
        let id=m["id"].as_str().or_else(||m["slug"].as_str())?;
        Some(json!({"id":id,"name":m["display_name"].as_str().or_else(||m["name"].as_str()).unwrap_or(id),"efforts":efforts(provider,id),"status":status,"cached":cached,"auth":auth}))
    }).collect::<Vec<_>>()))
}
#[tauri::command]
pub async fn ai_models(host: tauri::State<'_, Host>, provider: Option<String>) -> Result<Value, DbError> {
    let mut settings = read(&host)?;
    let selected = provider.unwrap_or_else(|| settings.provider.clone());
    Provider::new(&selected)?;
    #[cfg(debug_assertions)]
    if std::env::var_os("AXDB_AI_FIXTURE").is_some() {
        return catalog_models(&json!([{"id":if selected=="anthropic" {"claude-opus-5-5"} else {"gpt-6.1-sol"}}]), &selected, "connected", true, "api_key");
    }
    if selected != settings.provider {
        settings.provider=selected.clone();
        settings.auth=if key(&selected)?.is_some() { "api_key" } else if selected=="openai" { "chatgpt_login" } else { "claude_login" }.into();
    }
    let path=host.paths.config.join(format!("ai-models-{selected}.json"));
    let cache=std::fs::read(&path).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
    let credential=Provider::new(&selected)?.refresh(&host,&settings).await;
    let status=if credential.is_ok() {"connected"} else if (selected=="openai" && oauth::accounts()?.as_array().is_some_and(|accounts| accounts.iter().any(|a|a["logged_in"]==true))) || (settings.auth=="claude_login" && cache.is_some()) {"expired"} else {"disconnected"};
    let fallback=|state: &str| -> Result<Value, DbError> {
        let default=json!([{"id":if selected=="anthropic" {"claude-opus-5-5"} else {"gpt-6.1-sol"}}]);
        catalog_models(cache.as_ref().map(|v|&v["models"]).unwrap_or(&default), &selected,state,true,&settings.auth)
    };
    let Ok(credential)=credential else { return fallback(status); };
    if let Some(cache)=&cache
        && now_seconds().saturating_sub(cache["fetched_at"].as_u64().unwrap_or(0)) < 3600 { return fallback(status); }
    let fetched=async {
        let mut request=oauth::client()?.get(if selected=="openai" {"https://api.openai.com/v1/models"} else {"https://api.anthropic.com/v1/models"});
        request=match &credential {
            Credential::Bearer(token)=>request.bearer_auth(token),
            Credential::ApiKey(token) if selected=="openai"=>request.bearer_auth(token),
            Credential::ApiKey(token)=>{
                let mut header=reqwest::header::HeaderValue::from_str(token).map_err(|_|DbError::query("Invalid AI credential"))?;
                header.set_sensitive(true); request.header("x-api-key",header)
            }
        };
        if selected=="anthropic" {request=request.header("anthropic-version","2023-06-01");}
        let value=oauth::json_response(request.send().await.map_err(|_|DbError::query("AI model catalog unavailable"))?).await?;
        catalog_models(&value,&selected,"connected",false,&settings.auth)
    }.await;
    match fetched {
        Ok(models)=>{
            storage::write_private(&path,&serde_json::to_vec(&json!({"fetched_at":now_seconds(),"models":models})).map_err(|_|DbError::internal("Cannot encode AI catalog"))?)?;
            Ok(models)
        }
        Err(error)=>fallback(if error.message.contains("HTTP 401") || error.message.contains("HTTP 403") {"expired"} else {status}),
    }
}
fn now_seconds()->u64 { time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effort_mapping_rejects_unknown_models_and_omits_unsupported_parameters() {
        assert!(efforts("openai","gpt-6.1-sol").contains(&"xhigh"));
        assert!(!efforts("anthropic","claude-opus-4-5").contains(&"max"));
        assert!(efforts("openai","gpt-4.1").is_empty());
        let settings=Settings{provider:"openai".into(),model:"gpt-4.1".into(),effort:"".into(),..Default::default()};
        assert!(request_body(&settings,json!({}),"hello",&[]).get("reasoning").is_none());
        let models=catalog_models(&json!({"data":[{"id":"gpt-6.1-sol"}]}),"openai","connected",false,"api_key").unwrap();
        assert_eq!(models[0]["efforts"][3],"xhigh");
        let mut settings=Settings{provider:"openai".into(),model:"gpt-6.1-sol".into(),effort:"xhigh".into(),..Default::default()};
        assert!(validate(&settings).is_ok());
        assert_eq!(request_body(&settings,json!({}),"hello",&[])["reasoning"]["effort"],"xhigh");
        settings.provider="anthropic".into();settings.model="claude-opus-4-5".into();settings.effort="max".into();
        assert!(validate(&settings).is_err());
        let cached=catalog_models(&models,"openai","expired",true,"chatgpt_login").unwrap();
        assert_eq!(cached[0]["status"],"expired");assert_eq!(cached[0]["cached"],true);
        assert_eq!(cached[0]["id"],models[0]["id"]);
    }
    #[test]
    fn anthropic_tools_preserve_adaptive_thinking_without_unsupported_forced_choice() {
        let body = request_body(
            &Settings::default(),
            json!({"schema":"users(id)"}),
            "Suggest",
            &[json!({"name":"propose_operation","input_schema":{"type":"object"}})],
        );
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["tool_choice"]["type"], "auto");
        assert!(body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Use the propose_operation tool"));
    }
    #[test]
    fn usage_sums_completed_response_tokens_per_provider_and_rejects_corrupt_storage() {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::new(storage::Paths {
            config: dir.path().into(),
            data: dir.path().into(),
            log: dir.path().into(),
        });
        record_usage(
            &host,
            "openai",
            &json!({"input_tokens":12,"output_tokens":3}),
        )
        .unwrap();
        record_usage(
            &host,
            "openai",
            &json!({"input_tokens":8,"output_tokens":5}),
        )
        .unwrap();
        record_usage(
            &host,
            "anthropic",
            &json!({"input_tokens":4,"output_tokens":1}),
        )
        .unwrap();
        let totals = usage(&host).unwrap();
        assert_eq!(totals["openai"]["input_tokens"], 20);
        assert_eq!(totals["openai"]["output_tokens"], 8);
        assert_eq!(totals["anthropic"]["input_tokens"], 4);
        std::fs::write(dir.path().join("ai-usage.json"), "[]").unwrap();
        assert!(usage(&host).is_err());
    }
    #[tokio::test]
    async fn openai_http_uses_bearer_responses_tools_and_redacted_failures() {
        use axum::{
            extract::Json,
            http::{HeaderMap, StatusCode},
            routing::post,
            Router,
        };
        let app = Router::new()
            .route(
                "/v1/responses",
                post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                    assert_eq!(headers["authorization"], "Bearer test-only-credential");
                    assert!(headers.get("x-api-key").is_none());
                    assert!(headers.get("anthropic-version").is_none());
                    assert_eq!(body["store"], false);
                    assert_eq!(body["stream"], true);
                    assert_eq!(body["tools"][0]["type"], "function");
                    assert_eq!(body["tool_choice"], "required");
                    (
                        [("content-type", "text/event-stream")],
                        include_str!("../../tests/fixtures/ai-openai.sse"),
                    )
                }),
            )
            .route(
                "/failed",
                post(|| async {
                    (
                        StatusCode::UNAUTHORIZED,
                        "test-only-credential echoed by provider",
                    )
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let settings = Settings {
            provider: "openai".into(),
            model: "gpt-6.1-sol".into(),
            ..Default::default()
        };
        let tools = [
            json!({"name":"propose_operation","description":"Propose only","input_schema":{"type":"object"}}),
        ];
        let body = request_body(&settings, json!({"schema":"users(id)"}), "suggest", &tools);
        let credential = Credential::Bearer("test-only-credential".into());
        let reply = super::super::send(
            &format!("http://{addr}/v1/responses"),
            &credential,
            body.clone(),
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(reply["tools"][0]["name"], "propose_operation");
        let error = super::super::send(&format!("http://{addr}/failed"), &credential, body, |_| {
            Ok(())
        })
        .await
        .unwrap_err();
        assert!(error.message.contains("401"));
        assert!(!error.message.contains(credential.secret()));
        server.abort();
    }
    #[tokio::test]
    async fn real_openai_only_when_explicitly_enabled() {
        let Ok(mode) = std::env::var("AXDB_TEST_OPENAI") else {
            return;
        };
        let auth = match mode.as_str() {
            "key" | "1" => "api_key",
            "login" => "chatgpt_login",
            _ => panic!("AXDB_TEST_OPENAI must be key or login"),
        };
        let settings = Settings {
            provider: "openai".into(),
            model: "gpt-6.1-sol".into(),
            auth: auth.into(),
            ..Default::default()
        };
        let credential = credential(&settings, tempfile::tempdir().unwrap().path())
            .await
            .unwrap();
        let body = request_body(&settings, json!({"schema":""}), "Reply exactly OK", &[]);
        let reply = send("openai", &credential, body, &mut |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(reply["stop_reason"], "completed");
    }
    #[tokio::test]
    async fn real_anthropic_only_when_explicitly_enabled() {
        let Ok(mode) = std::env::var("AXDB_TEST_ANTHROPIC") else {
            return;
        };
        let _guard = ant_test_lock().lock().await;
        let auth = match mode.as_str() {
            "key" | "1" => "api_key",
            "login" => "claude_login",
            _ => panic!("AXDB_TEST_ANTHROPIC must be key or login"),
        };
        let settings = Settings {
            auth: auth.into(),
            ..Default::default()
        };
        let credential = credential(&settings, tempfile::tempdir().unwrap().path())
            .await
            .unwrap();
        let body = request_body(&settings, json!({"schema":""}), "Reply exactly OK", &[]);
        let reply = send("anthropic", &credential, body, &mut |_| Ok(()))
            .await
            .unwrap();
        assert!(reply["stop_reason"].is_string());
    }
}
