fn main() {
    axdb_driver_sdk::serve(driver_postgres::PostgresDriver::new());
}
