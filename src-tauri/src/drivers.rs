use dbcore::Registry;
use std::sync::Arc;

pub fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(Arc::new(driver_postgres::PostgresDriver::new()));
    registry.register(Arc::new(driver_neo4j::Neo4jDriver::new()));
    registry.register(Arc::new(driver_sqlite::SqliteDriver::new()));
    registry.register(Arc::new(driver_mongodb::MongoDriver::new()));
    registry
}
