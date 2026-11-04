# ETL 처리량·메모리 측정 (2026-11-04, 자체 측정)

측정 코드: `src-tauri/tests/etl_bench.rs` (`#[ignore]`). 재현:

```bash
AXDB_ETL_BENCH_ROWS=100000 AXDB_TEST_PG="host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test" \
  cargo test -p axdb --release --test etl_bench -- --ignored --nocapture --test-threads=1
# 드라이버를 앱처럼 별도 프로세스로: AXDB_DRIVER_DIR=<target>/release 를 함께 준다
```

## 조건

- 환경: WSL2(Linux 6.18) 위 Docker, `rust:1.99.1` 기반 빌드 이미지, release 프로파일(LTO). PostgreSQL 17 공식 이미지, 같은 호스트, 기본 설정. 단일 실행값이며 3회 중앙값이 아니다.
- 데이터: 주문 테이블 7열(id, customer, email, amount, status, created_at, note 80자 hex). 행당 평균 약 155바이트. SQLite 재귀 CTE로 seed(10만 행 0.2~0.3초, 100만 행 3.05초).
- 경로: 테이블 소스(`browse` offset 페이지) → 엔진 → `import_rows_progress`(PostgreSQL은 COPY). 엔진 단위 측정이라 Tauri 명령·이벤트 비용은 빠진다.
- 기준선: 같은 엔진 안의 `CREATE TABLE … AS SELECT`. ETL 경로가 넘을 수 없는 상한이다.
- 최대 RSS: `/proc/self/clear_refs`로 사례마다 초기화한 뒤 읽은 VmHWM(테스트 프로세스). 같은 프로세스 드라이버면 드라이버 메모리가 포함되고, 드라이버 프로세스 경로에서는 호스트 쪽(엔진·IPC 버퍼)만 잡힌다.

## 결과

| 사례 | 10만 행 · 같은 프로세스 | 10만 행 · 드라이버 프로세스 | 100만 행 · 같은 프로세스 | 100만 행 · 드라이버 프로세스 |
|---|---|---|---|---|
| SQLite 네이티브 CTAS (상한) | 1,062,865 rows/s | 1,488,304 | 970,667 | 2,333,378 |
| SQLite→SQLite 배치 1,000 | 25,691 rows/s · 29 MiB | 18,498 · 14 MiB | **10,357** · 47 MiB | 14,212 · 12 MiB |
| SQLite→SQLite 배치 5,000 | 35,345 · 59 MiB | 32,985 · 27 MiB | 32,583 · 62 MiB | 20,218 · 27 MiB |
| SQLite→SQLite 배치 10,000 | 42,697 · 94 MiB | 35,114 · 45 MiB | 38,996 · 96 MiB | 14,212 · 12 MiB0 |
| SQLite→SQLite 변환 4개 + create_table, 배치 5,000 | 50,121 · 94 MiB | 34,618 · 45 MiB | 39,495 · 82 MiB | 27,124 · 25 MiB |
| SQLite→PostgreSQL(COPY), cast 2개, 배치 5,000 | 46,524 · 38 MiB | 31,819 · 27 MiB | 42,960 · 44 MiB | 33,312 · 27 MiB |
| PostgreSQL→PostgreSQL, 배치 10,000 | 70,539 · 55 MiB | 39,077 · 46 MiB | **33,625** · 58 MiB | 32,786 · 46 MiB |
| PostgreSQL 네이티브 CTAS (상한) | 1,016,811 | 1,237,135 | 356,451 | 1,271,740 |

변환 사례의 rows/s는 읽은 행 기준이다(필터가 25%를 걸러 쓰는 행이 적다).

## 해석

1. **메모리는 행 수가 아니라 배치 크기를 따른다.** 100만 행에서도 최대 RSS는 44~96 MiB였다. 전체 결과를 올리지 않는 설계가 동작한다. 배치 10,000은 1,000보다 약 50 MiB를 더 쓴다.
2. **offset 페이지 조회의 제곱 비용이 보인다.** SQLite 배치 1,000은 10만 → 100만 행에서 25.7k → 10.4k rows/s, PG→PG는 70.5k → 33.6k rows/s로 떨어졌다. 페이지마다 앞선 행을 다시 건너뛰기 때문이다. 배치를 키우면 페이지 수가 줄어 완화된다(배치 5,000 이상 권장). 근본 해결은 드라이버의 keyset(커서) 조회다.
3. **SQLite 적재는 행마다 문장을 다시 준비한다.** SQLite 드라이버의 `import_rows`는 행마다 `prepare("INSERT … RETURNING *")`를 하고 행마다 SQL 문자열을 결과에 담는다. 배치 크기에 비례한 메모리 증가와 같은 엔진 상한 대비 2~4% 처리량이 이와 맞는다.
4. **드라이버 프로세스 경계(MessagePack IPC) 비용**은 사례에 따라 0~45%였다(100만 행 SQLite 배치 5,000: 32.6k → 20.2k rows/s). 앱의 실제 경로는 이쪽이며, 호스트 쪽 메모리는 12~46 MiB로 더 작다.
5. 참고로 조사한 경쟁 도구는 공식 rows/s를 공개하지 않는다([경쟁 조사](etl-competitors.md) §4). 유일한 단편 수치인 Hop 사용자 보고(SQL Server, 500만 행, 약 26k rows/s, 환경 불명)와 같은 규모지만 조건이 달라 우열 근거로 쓰지 않는다.

## 후속 작업

- SQLite 드라이버 적재 경로의 문장 재사용(`prepare_cached`)과 적용 결과의 SQL 문자열 축소 — 아래 개선 결과 참조.
- 드라이버 프로토콜에 keyset 커서 조회 추가(ETL 3단계).
