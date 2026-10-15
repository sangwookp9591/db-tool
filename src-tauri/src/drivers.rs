use dbcore::Registry;
use std::sync::Arc;

pub fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(Arc::new(driver_postgres::PostgresDriver::new()));
    registry
}
