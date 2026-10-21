#!/usr/bin/env bash
set -euo pipefail

prefix=axdb-qa-20261021
names=("$prefix-pg" "$prefix-mysql" "$prefix-mariadb" "$prefix-neo4j" "$prefix-mongo" "$prefix-redis")
ports=(55432 53306 53307 57687 57017 56379)

wait_port() {
  local name="$1" port="$2"
  for _ in $(seq 1 90); do
    if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then return 0; fi
    sleep 2
  done
  docker logs "$name" >&2 || true
  echo "Timed out waiting for $name on $port" >&2
  return 1
}

case "${1:-start}" in
  start)
    for port in "${ports[@]}"; do
      if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
        echo "Refusing to bind occupied loopback port: $port" >&2
        exit 1
      fi
    done
    for name in "${names[@]}"; do
      if docker inspect "$name" >/dev/null 2>&1; then
        echo "Refusing to reuse existing container: $name" >&2
        exit 1
      fi
    done
    docker run -d --name "$prefix-pg" -e POSTGRES_PASSWORD=testpw -e POSTGRES_DB=axdb_test -p 127.0.0.1:55432:5432 postgres:16
    docker run -d --name "$prefix-mysql" -e MYSQL_ROOT_PASSWORD=testmysql -e MYSQL_DATABASE=axdb_test -p 127.0.0.1:53306:3306 mysql:8
    docker run -d --name "$prefix-mariadb" -e MARIADB_ROOT_PASSWORD=testmysql -e MARIADB_DATABASE=axdb_test -p 127.0.0.1:53307:3306 mariadb:11
    docker run -d --name "$prefix-neo4j" -e NEO4J_AUTH=neo4j/testneo4j -p 127.0.0.1:57687:7687 neo4j:5
    docker run -d --name "$prefix-mongo" -e MONGO_INITDB_ROOT_USERNAME=axdb -e MONGO_INITDB_ROOT_PASSWORD=testmongo -p 127.0.0.1:57017:27017 mongo:7
    docker run -d --name "$prefix-redis" -p 127.0.0.1:56379:6379 redis:7 redis-server --requirepass testredis
    wait_port "$prefix-pg" 55432
    for _ in $(seq 1 90); do
      if docker exec "$prefix-pg" pg_isready -U postgres >/dev/null 2>&1; then break; fi
      sleep 2
    done
    docker exec "$prefix-pg" pg_isready -U postgres >/dev/null
    wait_port "$prefix-mysql" 53306
    wait_port "$prefix-mariadb" 53307
    wait_port "$prefix-neo4j" 57687
    wait_port "$prefix-mongo" 57017
    wait_port "$prefix-redis" 56379
    docker exec "$prefix-pg" psql -U postgres -d postgres -v ON_ERROR_STOP=1 -c 'CREATE DATABASE dbtool_test'
    docker exec "$prefix-pg" psql -U postgres -d dbtool_test -v ON_ERROR_STOP=1 -c "CREATE SCHEMA e2e; CREATE TABLE e2e.typed(id integer PRIMARY KEY, note text, raw bytea); INSERT INTO e2e.typed VALUES (1, NULL, decode('00ffabcd','hex')), (2, '한글 텍스트', NULL); CREATE TABLE e2e.no_pk(note text); CREATE TABLE e2e.big AS SELECT i AS id, (i % 2 = 0) AS flag FROM generate_series(1,250000) AS i; ALTER TABLE e2e.big ADD PRIMARY KEY(id); CREATE SCHEMA e2e_fk; CREATE TABLE e2e_fk.parent(id integer PRIMARY KEY); CREATE TABLE e2e_fk.child(id integer PRIMARY KEY, parent_id integer REFERENCES e2e_fk.parent(id));"
    echo "All isolated backend fixtures are accepting TCP connections."
    ;;
  env)
    cat <<'EOF'
export AXDB_TEST_PG='host=127.0.0.1 port=55432 dbname=axdb_test user=postgres password=testpw'
export AXDB_TEST_MYSQL=1
export AXDB_TEST_MARIADB=1
export AXDB_TEST_NEO4J='bolt://127.0.0.1:57687|neo4j|testneo4j'
export AXDB_TEST_MONGO='mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin'
export AXDB_TEST_REDIS='redis://:testredis@127.0.0.1:56379/0'
EOF
    ;;
  status)
    docker ps -a --filter "name=^${prefix}-" --format '{{.Names}} {{.Status}} {{.Ports}}'
    ;;
  stop)
    # Only remove the six containers whose exact names are reserved by this fixture runner.
    docker rm -f "${names[@]}"
    ;;
  *) echo "Usage: $0 {start|env|status|stop}" >&2; exit 2 ;;
esac
