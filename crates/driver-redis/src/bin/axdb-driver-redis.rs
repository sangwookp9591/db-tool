fn main() {
    axdb_driver_sdk::serve(driver_redis::RedisDriver::new());
}
