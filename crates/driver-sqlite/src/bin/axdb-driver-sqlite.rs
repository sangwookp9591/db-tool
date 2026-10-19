fn main() {
    axdb_driver_sdk::serve(driver_sqlite::SqliteDriver::new());
}
