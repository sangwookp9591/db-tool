apt-get update -qq >/dev/null && apt-get install -y -qq wine64 >/dev/null 2>&1
W=$(command -v wine64 || echo /usr/lib/wine/wine64); echo "wine: $W $($W --version 2>/dev/null)"
export WINEDEBUG=-all WINEPREFIX=/tmp/wp
$W wineboot -i >/dev/null 2>&1
export AXDB_TEST_PG="host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test" AXDB_TEST_NEO4J="bolt://127.0.0.1:57687|neo4j|testpassword" AXDB_TEST_MONGO="mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin" AXDB_TEST_REDIS="redis://:testredis@127.0.0.1:56379/0"
for e in $(cat /target-win/exes.txt); do n=$(basename $e); r=$(timeout 600 $W $e --test-threads=1 2>&1 | grep -E "^test result|panicked|error" | head -3 | tr '\n' ' '); echo "$n :: $r"; done
