//! Generate the checked-in wire examples from the real Rust serializers.
use dbcore::*;
use driver_postgres::PostgresDriver;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let driver = PostgresDriver::new();
    let session = driver
        .connect(&ConnectConfig {
            fields: [
                ("host".into(), "127.0.0.1".into()),
                ("port".into(), "55432".into()),
                ("database".into(), "dbtool_test".into()),
                ("user".into(), "postgres".into()),
            ]
            .into(),
            secret: std::env::var("DBTOOL_EXAMPLE_SECRET").ok(),
            connect_timeout_ms: 2000,
        })
        .await?;
    let request = |text: &str| ExecRequest {
        text: text.into(),
        params: Default::default(),
        row_limit: 1000,
        graph_node_limit: 300,
        timeout_ms: 5000,
    };
    let result=session.execute(&request("SELECT 9223372036854775807::int8 AS id, 12345678901234567890.123456789::numeric AS amount, '{\"big\":9223372036854775807}'::jsonb AS document, NULL::text AS missing"),CancelToken::new()).await?;
    let error = session
        .execute(
            &request("SELECT missing_example_column"),
            CancelToken::new(),
        )
        .await
        .unwrap_err();
    session.close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"manifest":driver.manifest(),"result":result[0],"error":error})
        )?
    );
    Ok(())
}
