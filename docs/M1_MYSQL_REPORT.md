# M1 — MySQL / MariaDB 설치형 드라이버

2026-10-19. `crates/driver-mysql`에 mysql_async 0.37.1/rustls 드라이버와 SDK 실행 파일을
추가했다. manifest id는 `mysql`, paradigm은 Relational이다. workspace/xtask 등록만으로
기존 호스트가 새 드라이버를 읽으며, 앱의 실행·권한·네트워크·UI에 MySQL driver-id 분기를
추가하지 않았다. `cargo tree --edges normal`에도 MySQL은 없고 호스트 dev-dependency만 있다.
AI 출력·Operation 승인 경로는 변경하지 않았다.

text protocol로 DECIMAL·UNSIGNED BIGINT·JSON 원문을 유지하고, 별도 연결의 KILL QUERY와
서버/클라이언트 제한 시간, lazy 탐색·PK 편집·paging·typed AND 필터·ERD·스냅숏·DDL 계획·
배치 가져오기·EXPLAIN/ANALYZE·계정/GRANT 계획·W4b byte-stream 백업을 구현했다.
[mysql_async 문서](https://docs.rs/mysql_async/0.37.1/mysql_async/)와
[MySQL 8.4 EXPLAIN](https://dev.mysql.com/doc/refman/8.4/en/explain.html)을 기준으로 선택했다.

## 실제 패키지 추가 검증

로컬 테스트 키로 서명한 실제 MySQL 실행 파일 패키지를 만들어 기존 package 검증·stage·
PluginDriver 핸드셰이크·Registry 등록을 통과시켰다. 기존 Installer에서도 미확인 서명의
명시적 동의 전 거부 → 동의 후 활성 marker 생성 → 등록된 드라이버로 실제 DB ping →
삭제와 marker/Registry 정리까지 통과했다. 테스트 설치는 임시 폴더에만 수행했다.

로컬 산출물은 `target-m1/artifacts/mysql-test-0.2.0.axdriver`(12,327,119 bytes)이며
SHA-256은 `5d9d3736beb74de99d5a5e9b2c82c2341a42183ffa22cf321dcb7c80d34723da`다. fixture 키로 서명한 **테스트용 native debug 패키지**이며
공식 서명·공증·게시 산출물이 아니다. 이 키는 릴리스 권한이 없다.

추가 조율 범위로 DriverManifest, package Manifest/CatalogEntry, xtask 및 공식 드라이버에
optional `icon_key`를 넣었다. None은 직렬화에서 생략해 기존 카탈로그 서명 바이트를
유지한다. UI의 기존 D5 키와 일치하며 새 UI 분기는 추가하지 않았다. **이미 배포된 구버전
호스트의 package schema가 deny_unknown_fields이면 새 icon_key metadata를 거부할 수
있다.** 이 metadata를 넣은 카탈로그의 구버전 앱 호환은 별도 배포 판단이 필요하다.

## 검증

모든 Rust 빌드는 `CARGO_TARGET_DIR=target-m1 CARGO_BUILD_JOBS=4`로 실행했다.
`AXDB_TEST_MYSQL=1`, `AXDB_TEST_MARIADB=1`은 각각 127.0.0.1:53306/53307의 고정된
테스트 컨테이너만 활성화한다. 초기 R2 중지 지시를 따랐고 재개 통보 후 검증했다.

- MySQL 8.4.11 / MariaDB 11.8.9: unit 8개 + 실DB 계약 3개 통과.
- driver-package unit 7개 통과: 이전 icon metadata/서명 호환과 값 변조 거부 포함.
- driver-mysql/driver-package all-targets `clippy -- -D warnings` 통과.
- `cargo build --workspace --bins` 통과 후 `AXDB_DRIVER_DIR=target-m1/debug` 플러그인 계약 3개 통과.
- 실제 MySQL 서명 패키지 검증·등록 1개, 실제 Installer 설치·연결·삭제 1개 통과.
- 기존 Installer 회귀 6개 통과, 기존 수동 검사 2개 ignored.
- UI `tsc --noEmit` 통과; 전체 `pnpm --dir ui test`: 36 suites / 141 tests 통과, 기존 1 suite / 1 test skipped. 병렬 UI 변경을 감지해 최종 상태에서 두 검사 모두 재실행했고 같은 결과를 확인했다.
- 변경 범위 `git diff --check` 통과.

실DB 계약은 값·NULL·BLOB/BIT/ENUM/SET, named parameter 주입 방어, JSON/정밀 decimal 필터,
특수 식별자, 다중 결과/뒤쪽 오류, 마지막 페이지, FK, DDL 계획의 비실행, rollback preview,
편집 충돌 전체 rollback, 1,001행 batch import, read-only 우회 거부, 열린 사용자 트랜잭션
보존, cancel/50ms timeout 후 재사용, 백업 payload의 정확한 데이터 재생, 잠긴 계정/GRANT
행렬을 포함한다. 백업 재생은 테스트 DB의 직접 SQL 검증이며 앱 복원 지원을 뜻하지 않는다.

TLS require의 cipher, verify-full의 사설 CA + `tls_server_name` 성공, 이름 불일치·잘못된
인증 거부를 직접 및 플러그인에서 확인했다. 기존 network fixture의 인증서 helper를
재사용해 **테스트 MySQL에만** SAN 인증서를 설정했다. 공개 CA 경로는 임시
`/tmp/axdb-m1-tls-root`가 가리키는 디렉터리의 `ca.crt`이며 테스트 시
`AXDB_TEST_MYSQL_CA`로 전달했다. 사용자 인증서/SSH는 사용하지 않았다.

발견 후 수정한 실패는 서버 제한 뒤 BENCHMARK의 정상 행 반환(deadline 우선순위·완료
시각 검사로 해결), workspace가 rustls provider 둘을 켰을 때의 TLS 패닉(기존 Neo4j와
같이 연결 전에 provider 선택), 식별자 내부 문구까지 바꾸는 SQL 치환(AST 변환)이다.
verify-ca는 mysql_async의 최신 rustls 이름 오류 문자열 처리 문제로 제공하지 않는다.
검증을 끄는 우회 대신 verify-full의 실제 CA/이름 검증을 유지했다.

## 100,000행 측정

MySQL 8.4.11 local plaintext 연결, native macOS debug, BIGINT + 32자 문자열의 2컬럼이다.
한 번 warm-up 후 교차 순서 3쌍의 중앙값이며, 앱/SDK와 같은 mimalloc purge_delay=0을
사용했다. 경계는 Session ResultSink의 첫 200행 청크와 완료이고 GUI paint는 포함하지 않는다.
DB 수집이 끝난 뒤 SDK가 전송하는 현재 구조다.

| 경계 | in-process | plugin |
|---|---:|---:|
| 첫 행 청크 | 420.56 ms | 420.44 ms |
| 전체 전달 | 441.98 ms | 728.93 ms |

첫 청크 차이는 -0.03%로 이 debug 조건에서는 ADR-0003의 +20% 이내다.
전체 전달은 +64.92%로 IPC/역직렬화 비용이 남는다. 작업 후 자식 프로세스의
`footprint`는 **14.34 MiB**, kernel peak **67.08 MiB**였고 RSS는 90.59 MiB였다.
RSS를 physical footprint로 대신하지 않았다. release·TLS/SSH 대량 결과·실제 UI paint에
이 수치를 일반화하지 않는다.

원자료: [앱과 동일 allocator](../bench/results-m1-mysql.json),
[초기 System allocator 비교](../bench/results-m1-mysql-system-allocator.json).
초기 비교는 부모 System/자식 mimalloc 조건이 달라 별도로 보존했다. 측정 하네스의 allocator를
보정한 뒤 다시 측정했으며, 초기 결과를 삭제하거나 통과 결과로 덮지 않았다.

## 지원 경계와 남은 확인

`graph_expand=false`이며 나머지 capability에는 구현 경로가 있다. 세부 작업이 안전하지
않으면 Unsupported로 거부한다. 데이터 보호 작업은 별도 UTC 연결을 쓰며, 열린 사용자
트랜잭션/autocommit=0을 먼저 거부한다. 현재 DATABASE()만 복사하고 임시 테이블·세션
변수는 복사하지 않는다. InnoDB base table, 트리거 없음과 메타데이터 가시성이 필요하고
stored/UDF 함수는 거부한다. AUTO_INCREMENT 간격과 잠금은 rollback으로 복원되지 않는다.

MySQL DDL preview, 새 테이블 생성과 import 묶음, atomic DDL migration, MariaDB ANALYZE,
자동 GRANT 적용, 앱의 MySQL 복원은 지원하지 않는다. GRANT는 `apply_supported=false`의
계획만 반환한다. MySQL ANALYZE는 8.0.18 이상에서 rollback 경로만 사용한다.
백업은 export만 제공하며 비-InnoDB·트리거·루틴·이벤트가 있으면 부분 성공으로 위장하지
않고 거부한다. SELECT/TRIGGER/EVENT/SHOW VIEW/EXECUTE 가시성을 요구하고 계정·GRANT·
서버 설정은 백업 대상이 아니다. 동시 DDL이 없는 조건의 InnoDB data snapshot이다.

MariaDB JSON alias는 browse에서 Json, 일반 SQL에서는 서버 LONGTEXT metadata에 따라
Text일 수 있으며 원문은 보존된다. 일반 SQL의 TIMESTAMP는 세션 타임존 값으로 표시하고
UTC가 보장되는 browse/보호 실행에서만 +00:00을 붙인다. DateTimeTz 파라미터는 UTC
세션을 확인한다. DECIMAL parameter/filter는 지수 없는 문자열, precision 65/scale 30
범위이며 컬럼 scale로 반올림해 비교하지 않는다. no-op UPDATE도 영향 행 수 0이면 Conflict로
rollback한다. PK 없는 테이블의 update/delete는 거부한다. 스냅숏에는 SHOW CREATE의
인덱스·제약을 포함하고 volatile AUTO_INCREMENT counter는 제외한다. 복잡한 옵션/인덱스
동시 변경은 명시적 migration이 필요하다. 권한 행렬은 명시적 table GRANT이며 유효 권한
전체나 role inheritance를 계산하지 않는다.

전체 Rust workspace test, cross-platform/release 패키지, 공식 키 서명·공증·배포, 사용자
앱/GUI·사용자 DB·사용자 SSH 검증은 실행하지 않았다. dist/target/target-rel, commit/push는
건드리지 않았다. 시작 때 이미 Cargo.lock/dbcore/기존 driver/src-tauri/UI 등에 병렬 변경이
있었고 기존 hunk를 보존했다. 추가 공유 변경은 icon metadata와 필요한 테스트 fixture다.
실패한 회귀 실행이 남긴 본 작업 소유 임시 schema는 정리했고 테스트 컨테이너 두 개는
후속 검증용으로 유지했다.
