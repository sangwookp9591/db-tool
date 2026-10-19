# C1 — 0.2.0 드라이버 카탈로그 하위 호환

2026-10-19. 작업 시작 시 Git 변경 사항은 없었다. ADR-0004·ADR-0003·ARCHITECTURE·README를 읽고 패키징과 카탈로그 호환 경로만 수정했다. AI 실행·권한 정책, 사용자 앱·GUI·DB·SSH 호스트는 건드리지 않았고 commit/push나 원격 게시는 하지 않았다. 빌드 산출물은 `target-c1`과 테스트 임시 폴더에만 생성했다.

패키지 `Manifest.icon_key`는 `Option<String>`으로 역직렬화하되 `skip_serializing`하여 새 `.axdriver`의 manifest는 v0.2.0 스키마를 유지한다. UI의 `icon_key` 사용은 유지하며 설치 후 브랜드 아이콘은 기존 handshake의 `dbcore::DriverManifest`에서 받는다. `DriverLogo`는 카탈로그에 아이콘 키가 없으면 이미 패러다임 기본 아이콘으로 표시하므로 UI 코드를 바꾸지 않았다.

`xtask package-drivers`의 실제 산출물 작성 경로를 회귀 테스트와 공유한다. `index.json`은 아이콘 키 없는 v1이며 `index-v2.json`은 항목별 아이콘 키를 포함한다. 두 카탈로그는 같은 키로 각각 서명한다. 0.3.0 호스트는 URL 상수의 v2를 먼저 조회하고 404/410에서만 v1으로 폴백한다. 서버 오류·잘못된 JSON·서명 변조는 폴백하지 않는다. 캐시의 ETag를 원본 URL에 묶어 v1과 v2의 ETag가 같아도 잘못된 304로 v2 전환을 놓치지 않게 했으며, URL 정보가 없는 기존 캐시는 한 번 다시 받는다. 오프라인 빌드 스냅숏은 `dist/drivers/index-v2.json`을 읽는다.

게시 스크립트는 두 카탈로그가 존재하고 아이콘 키를 제외한 내용이 일치하는지 확인한 후 패키지 무결성을 검사한다. 기존처럼 패키지 먼저, 카탈로그 마지막 순서이며 마지막 `drivers-index` 업로드에 두 JSON과 `--clobber`를 함께 전달한다. 실제 GitHub 작업은 실행하지 않았다.

`Manifest`, `CatalogEntry`, `Catalog`, `SignedCatalog`의 `deny_unknown_fields`를 유지했다. 현재 카탈로그 서명 검증은 타입으로 파싱한 뒤 재직렬화한 바이트를 검증하므로 알 수 없는 필드를 무시하면 그 필드는 인증되는 바이트에서 빠진다. 단순히 strict 파싱만 제거하면 미래의 보안 의미를 가진 필드를 구버전이 조용히 무시하는 문제가 생길 수 있다. 따라서 이번 v2에서는 엄격한 파싱을 유지하며, 향후 확장이 필요하면 원본 JSON 전체를 인증하는 정규화 형식과 필수 확장/버전 협상을 먼저 설계하고 새 버전 URL로 도입하는 것을 권장한다.

검증한 항목:

- `CARGO_TARGET_DIR=target-c1 cargo build --workspace --bins`: 통과.
- `CARGO_TARGET_DIR=target-c1 cargo test -p axdb-driver-package -p xtask`: 패키지 8개·xtask 1개 통과. 패키지 icon 키 누출 테스트의 수정 전 실패와 수정 후 성공을 확인했다.
- `xtask/tests/fixtures/v0_2_0.rs`: `git show d07c12f:crates/driver-package/src/lib.rs`에서 6개 구조체 정의를 그대로 복사했으며 원본과 문자열 일치도 확인했다. 6개 공식 드라이버(MySQL/MariaDB 포함), 5개 플랫폼의 검증용 실행 파일 바이트로 실제 xtask 산출물 작성 함수를 호출해 v1 카탈로그·manifest의 구버전 파싱, 카탈로그/manifest 서명, 패키지/실행 파일 SHA-256, manifest의 구버전 재직렬화 바이트 일치를 검증했다. v2 서명 검증·아이콘 변조 거부·알 수 없는 필드 거부도 확인했다.
- `CARGO_TARGET_DIR=target-c1 cargo test -p axdb --lib driver_install`: 8개 통과·2개 ignored. 이 첫 실행의 MySQL 환경 의존 테스트는 입력 파일이 없어 본문을 실행하지 않았으므로 실제 설치 근거로 사용하지 않는다. 별도 실제 실행 결과는 아래에 기록한다.
- `CARGO_TARGET_DIR=target-c1 cargo clippy -p axdb-driver-package -p xtask -p axdb --all-targets -- -D warnings`: 통과.
- `pnpm --dir ui exec tsc --noEmit`: 통과. `pnpm --dir ui test`: 37개 파일·144개 테스트 통과, 1개 파일·1개 테스트 skipped.
- `python3 scripts/test-publish-drivers.py`: fake gh를 이용한 격리 테스트 4개 통과. 실제 원격 호출 없음. `bash -n scripts/publish-drivers.sh`, 변경 Rust 파일의 `rustfmt --check`, `git diff --check`: 통과.
- 실제 MySQL 실행 파일 검증: `env -u AXDB_TEST_MYSQL CARGO_TARGET_DIR=target-c1 AXDB_DRIVER_DIR=$PWD/target-c1/debug AXDB_MYSQL_PACKAGE_OUT=/tmp/axdb-c1-mysql.axdriver cargo test -p axdb --test mysql_package signed_mysql_package_loads_without_host_driver_registration` 통과(1개). 테스트 키로 만든 패키지 manifest에는 아이콘 키가 없고, 설치 후 실제 SDK handshake의 아이콘 키는 `mysql`인 것을 확인했다.
- 실제 호스트 설치 경로: 위와 같은 환경에 `AXDB_PACKAGE_TEST_BINARY=$PWD/target-c1/debug/axdb-driver-example-kv`를 추가해 `cargo test -p axdb --lib driver_install -- --include-ignored --skip resource_bench_install_signed_packages` 실행. 9개 통과·0 ignored(리소스 전용 하네스는 필터로 제외). MySQL의 미확인 서명 동의·활성화·제거와 실제 SDK의 HTTP 카탈로그/ETag·설치·취소·실패 롤백·업데이트·삭제·설치 후 변조 거부를 실행했다. 모든 설치 경로는 임시 폴더를 사용했고 DB 연결 옵션은 명시적으로 해제했다.

최종 Git 변경은 이 작업의 기존 파일 12개 수정과 신규 파일 3개(보고서·xtask 테스트·구버전 fixture)이며 commit/push는 없다. 서명은 테스트 키만 사용했다. 기존 테스트 실패를 숨기거나 완료 주석으로 대체하지 않았으며, 실제 바이너리 테스트와 검증용 바이트를 쓰는 호환 테스트의 범위를 구분했다.

미검증: 공식 키로 만든 릴리스 산출물과 GitHub 게시, Developer ID 서명·공증, 5개 플랫폼 실제 크로스 빌드·실행, v0.2.0 앱 GUI E2E, DB 기능·MySQL/MariaDB 실서버 동작, 리소스 전용 하네스. 이번 수정은 패키지·카탈로그 계약이며 DB 쿼리를 변경하지 않는다. 코디네이터는 새 xtask로 패키지를 다시 생성하고 두 카탈로그를 함께 게시한 뒤 공식 릴리스 경로를 확인해야 한다. v2가 없는 서버에서는 정상 요청 1회에 더해 v1 폴백 요청 1회가 필요하다.
