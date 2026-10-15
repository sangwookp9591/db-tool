set -e
apt-get update -qq >/dev/null && apt-get install -y -qq libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev pkg-config >/dev/null
export AXDB_TEST_PG="host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test" AXDB_TEST_NEO4J="bolt://127.0.0.1:57687|neo4j|testpassword" AXDB_TEST_MONGO="mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin" AXDB_TEST_REDIS="redis://:testredis@127.0.0.1:56379/0"
. /env.sh; export AXDB_TEST_TLS_CA=/tls/ca.crt AXDB_TEST_TLS_WRONG_CA=/tls/wrong-ca.crt
cd /src && export CARGO_TARGET_DIR=/target
uname -srm
cargo build --release --locked -p axdb 2>&1 | tail -1
cargo test --workspace --locked --no-fail-fast 2>&1 | grep -E "^test result|panicked|FAILED" | awk '/panicked|FAILED/{print} /^test result/{p+=$4;f+=$6;g+=$8} END{print "TOTAL passed",p,"failed",f,"ignored",g}'
