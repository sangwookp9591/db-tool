//! Reuse the exact existing driver contracts with proxy-backed constructors.
// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
extern crate driver_mongodb as real_mongodb;
extern crate driver_neo4j as real_neo4j;
extern crate driver_postgres as real_postgres;
extern crate driver_mysql as real_mysql;
extern crate driver_redis as real_redis;
extern crate driver_sqlite as real_sqlite;
extern crate self as driver_mongodb;
extern crate self as driver_neo4j;
extern crate self as driver_postgres;
extern crate self as driver_mysql;
extern crate self as driver_redis;
extern crate self as driver_sqlite;
use async_trait::async_trait;
use axdb::plugin_host::PluginDriver;
use dbcore::*;
use std::sync::Arc;

macro_rules! proxy_driver {
    ($name:ident, $id:literal, $fallback:path) => {
        pub struct $name(Arc<dyn Driver>);
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl $name {
            pub fn new() -> Self {
                match std::env::var_os("AXDB_DRIVER_DIR") {
                    Some(dir) => {
                        let executable = std::path::PathBuf::from(dir).join(format!(
                            "axdb-driver-{}{}",
                            $id,
                            std::env::consts::EXE_SUFFIX
                        ));
                        let plugin = std::thread::spawn(move || {
                            tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .unwrap()
                                .block_on(PluginDriver::load(executable))
                        })
                        .join()
                        .unwrap()
                        .unwrap();
                        Self(Arc::new(plugin))
                    }
                    None => Self(Arc::new(<$fallback>::new())),
                }
            }
        }
        #[async_trait]
        impl Driver for $name {
            fn manifest(&self) -> &DriverManifest {
                self.0.manifest()
            }
            async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
                self.0.connect(cfg).await
            }
        }
    };
}
proxy_driver!(PostgresDriver, "postgres", real_postgres::PostgresDriver);
proxy_driver!(MysqlDriver, "mysql", real_mysql::MysqlDriver);

#[path = "../../crates/driver-mysql/tests/mysql.rs"]
mod mysql_contract;
proxy_driver!(Neo4jDriver, "neo4j", real_neo4j::Neo4jDriver);
proxy_driver!(SqliteDriver, "sqlite", real_sqlite::SqliteDriver);
proxy_driver!(MongoDriver, "mongodb", real_mongodb::MongoDriver);

#[path = "../../crates/driver-mongodb/tests/mongodb.rs"]
mod mongo;
#[path = "../../crates/driver-mongodb/tests/schema.rs"]
mod mongo_schema;
#[path = "../../crates/driver-neo4j/tests/neo4j.rs"]
mod neo4j;
#[path = "../../crates/driver-postgres/tests/postgres.rs"]
mod postgres;
#[path = "../../crates/driver-sqlite/tests/sqlite.rs"]
mod sqlite;

proxy_driver!(RedisDriver, "redis", real_redis::RedisDriver);

#[path = "../../crates/driver-redis/tests/redis.rs"]
mod redis_contract;

#[path = "../../crates/driver-postgres/tests/explain.rs"]
mod postgres_explain;
#[path = "../../crates/driver-neo4j/tests/explain.rs"]
mod neo4j_explain;
#[path = "../../crates/driver-sqlite/tests/explain.rs"]
mod sqlite_explain;
#[path = "../../crates/driver-mongodb/tests/explain.rs"]
mod mongo_explain;

#[path = "../../crates/driver-postgres/tests/management.rs"]
mod postgres_management;
#[path = "../../crates/driver-sqlite/tests/management.rs"]
mod sqlite_management;
#[path = "../../crates/driver-neo4j/tests/management.rs"]
mod neo4j_management;
#[path = "../../crates/driver-mongodb/tests/management.rs"]
mod mongo_management;
