# W6-rename-axdb: 제품명을 AxDB로 변경

이 작업은 반드시 끝까지 완료한다. 사용자 지시가 skill 지침보다 우선한다.

사용자 결정: 프로젝트 이름은 **AxDB**("AX DB Tool"의 뜻)이다.

## Change
기계적인 이름 변경이며, 동작은 바꾸지 않는다. 바꿀 대상은 다음과 같다.

- **Tauri 설정:** `productName`과 창 제목은 `AxDB`, `identifier`는 `dev.axdb.app`
- **호스트 crate와 바이너리:** 패키지 이름 `db-tool` → `axdb`. 이 이름을 참조하는 스크립트와 문서도 함께 고친다.
- **키체인:** 서비스명 `db-tool` → `axdb`
  - 기존 `db-tool` 항목은 마이그레이션하지 않는다. 사용자 데이터가 아직 없으므로 문서에 그 사실만 적는다.
- **환경 변수와 파일명:**
  - `DBTOOL_*` → `AXDB_*`(CONFIG_DIR, AGENT_BRIDGE, BENCH, TEST_PG, TEST_NEO4J 등)
  - 로그 파일 `db-tool.log` → `axdb.log`
- **UI:** 헤더 텍스트 `AxDB`, `<title>`, `window.dbtool` → `window.axdb`
  - WebMCP 도구 이름에 접두사가 있으면 함께 바꾼다.
  - **이전 이름 별칭은 두지 않는다.**
- **문서와 벤치 스크립트:** README, `docs/*`, `bench/*.mjs`
  - `docs/tasks/*`와 이전 보고서(`*-report.md`)는 기록 보존을 위해 그대로 둔다.
- **남은 이전 이름 확인:**
  - `grep -rniE "db-tool|dbtool" --exclude-dir={target,node_modules,.git,dist} --exclude-dir='target-*' .`의 결과에서 기록용 문서를 뺀 나머지가 0건이어야 한다.
  - 저장소 디렉터리 이름 자체는 바꾸지 않는다.

## Constraints
- 동작 변경 금지
- git commit 금지
- 사용자 데이터 DB 접속 금지. 테스트 DB만 사용한다.

## Observable acceptance
- `cargo build --workspace`, `cargo test --workspace` 통과(`AXDB_TEST_PG`, `AXDB_TEST_NEO4J` 사용)
- clippy `-D warnings` 통과
- `pnpm --dir ui build`와 vitest 통과
- 위 grep 결과 보고
- debug 앱을 띄웠을 때 창 제목이 `AxDB`이고 MCP 브리지 smoke가 통과
