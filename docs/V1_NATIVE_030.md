# V1 — 0.3.0 네이티브 통합 검증

2026-10-20 · 기준 main `480eeac` + 아래 작은 수정 3건. [ADR-0004](adr/0004-ax-native-workbench.md), [ADR-0003](adr/0003-installable-drivers.md), [ARCHITECTURE](ARCHITECTURE.md)를 계약으로 검증했다. **기능 검증은 통과했으며, 숨김 상태의 엄격한 CPU 0 조건은 실패했다.** AI 공급자 실호출과 OAuth는 미검증이다.

## 실행 환경과 증거 범위

`CARGO_TARGET_DIR=target-v1 cargo build --workspace --bins` 후 별도 debug 프로세스를 실행했다. `AXDB_DRIVER_DIR`은 `target-v1/debug`, `AXDB_CONFIG_DIR`은 `/tmp/axdb-v1-native-030`이다. 사용자 `/Applications/AxDB.app`, 사용자 DB·SSH 호스트, 공유 `dist/`·`ui/dist`·`target`·`target-rel`은 수정하지 않았다. 시작 작업 트리는 깨끗했고 commit/push는 하지 않았다.

PG 55432, Neo4j 57687, MongoDB 57017, Redis 56379, MySQL 53306, MariaDB 53307, SSH 55222, SOCKS 55080의 테스트 인프라를 사용했다. 터널 내부의 `host.docker.internal`은 이 테스트 서버를 가리킨다. Rust 네트워크 테스트에는 기존 테스트 전용 TLS 컨테이너도 포함된다.

UI는 현재 소스에서 `/tmp/axdb-v1-ui`로 빌드했다. 여기에만 테스트용 WebView 작업 전송 스크립트를 붙여 실제 native command와 DOM 입력·클릭을 실행했다. native 저장/열기 대화상자, MCP 승인 버튼, 캡처는 해당 프로세스 PID만 대상으로 제어했다. MCP는 실제 HTTP 브리지이며 DB 호출은 실제 플러그인 프로세스를 통과한다. 이 계측 경로는 배포 앱의 인터페이스가 아니다.

비밀번호는 기존 테스트 픽스처에서 분리한 debug 전용 비공개 secret store로 공급했다. 실제 OS 키체인 검증으로 간주하지 않는다. mock updater 피드만 루프백으로 교체했고 다운로드·설치하지 않았다. CPU 측정 전에는 테스트 폴링을 중지했다. 최종 실행 파일 해시와 계측 조건은 [receipts.json](verification/v1-native-030/receipts.json)에 있다.

## 항목별 결과

| 항목 | 결과 | 실제 확인한 내용 |
|---|---|---|
| 기존 `e2e-bridge.mjs` | 통과 | 21/21. PG 정확한 큰 정수·decimal·NULL·시간·JSON, 탐색·페이지·상한·취소·재사용, Neo4j graph/table·경로·확장·읽기 전용 |
| 기존 `preview-e2e.mjs` | 통과 | 5/5. PG UPDATE·DDL, Neo4j CREATE, SQLite DELETE 미리보기 후 비커밋; 비롤백 문장 거부 |
| 기존 `permissions-e2e.mjs` | 통과 | 13/13. 실제 native 승인·거절, 승인 대기 중 폐기, UI 읽기 전용, 토큰 회전·만료·감사 로그. 기존 셀렉터 수정 불필요 |
| 드라이버 설치 | 통과 | 공식 서명된 로컬 `sqlite-0.2.0.axdriver`를 0.3.0 앱에서 설치·handshake; 설치됨 화면에서 0.2.0 패키지 확인 |
| MySQL·MariaDB | 통과 | 각각 실제 연결·탐색·쿼리·정규화 실행 계획. MySQL native 계획 탭도 캡처 |
| SSH 연결 | 통과 | native 지문 확인·신뢰, DNS→SSH→터널→DB 인증→권한 단계, 터널을 통한 PG 쿼리. 이 평문 픽스처의 TLS 단계는 skipped |
| SOCKS5 연결 | 통과 | 실제 native 단계별 테스트·PG 연결·쿼리·종료 |
| CSV 가져오기 | 통과 | native 명령과 UI 두 경로. 컬럼 매핑→미리보기 비커밋→승인→2행 적용; 승인 영수증 재사용 거부 |
| 스키마 비교 | 통과 | 실제 PG snapshot에 추가 객체를 제안, diff 트리·DDL 생성. 제안한 테이블이 DB에 생성되지 않았음을 조회 |
| 백업·새 DB 복원 | 통과 | native 저장 창으로 v2 `.axbackup` 생성, native 열기 창으로 검증, 승인 없는 적용 거부, 새 빈 PG DB에 복원 후 원본 2행+가져온 2행 확인 |
| 새 DB 복원의 UI 범위 | 통과 | PG는 `new_database=false`를 선언하므로 새 빈 DB를 먼저 생성·연결하고 native 복원 명령을 호출했다. UI의 “새 DB 생성” 기능을 검증한 것은 아니다 |
| 권한 행렬·계획 | 통과 | PG 역할·행렬 표시, SELECT 셀 토글→GRANT 계획·롤백 미리보기. 실제 권한은 false로 유지. 권한 적용은 요청 범위 밖 |
| MCP 클라이언트·페이지 | 통과 | native 클라이언트 생성, 2행/8 KiB 예산, `get_result_page`, 큰 UTF-8 셀의 byte page, 다른 소유자 거부, 연결 종료 후 handle 거부 |
| 연결 해제 | 통과 | 기존 E2E에서 탭·세션 종료; 종료한 세션의 캐시 결과를 다시 읽지 못함 |
| 표·ERD·연결 탐색 | 통과 | PG 표, FK ERD, child의 참조 버튼으로 필터된 parent 행 이동 |
| 그래프·문서·키-값 | 통과 | 실제 Neo4j 그래프/표, MongoDB BSON 문서 트리, Redis SCAN 키 결과. 문서는 실제 1행 픽스처의 ObjectId·Int64·Decimal128·중첩 값 사용 |
| 편집 미리보기·읽기 전용 회귀 | 통과 | PK 편집 미리보기 후 값 보존; MCP 쓰기 차단과 열린 연결의 UI 읽기 전용 강제 |
| 변경 로그 | 통과 | 0.3.0 기록·현재 빌드 `v0.3.0@480eeac` 표시 |
| 업데이트 확인 | 통과 | 실제 native updater가 모의 피드의 `0.3.1-v1-test` 감지. 최종 피드 영수증: 확인 2회, 다운로드 0회 |
| 두 테마 캡처 | 통과 | 실제 native 창 35장. 드라이버·PG 표/계획/ERD·MySQL 계획·그래프·문서·키-값·가져오기·diff·권한·백업·MCP·변경 로그·업데이트 |
| 유휴 애니메이션 | 통과 | 표시/숨김 모두 `document.getAnimations()`의 running 항목 0, 숨김에서 `document.hidden=true` 확인 |
| 표시 상태 CPU 0 | 통과 | 폴링 중지 후 약 15초. host·WebKit 3개·드라이버 6개 모두 누적 CPU 증가 0.00초 |
| 숨김 상태 CPU 0 | **실패** | 약 15초. Mongo 드라이버만 +0.01초(0.0667%), 나머지는 0.00초. 엄격한 0 기준을 충족하지 못함 |
| AI 실호출·OAuth | 미검증 | 계정을 사용하지 않았다. 테스트의 계획/승인 경계 통과를 실제 공급자 호출 증거로 확대하지 않음 |
| OS 키체인·클라우드 IAM 실인증 | 미검증 | debug secret store 사용. 실제 계정·OS 키체인 테스트는 실행하지 않음 |
| 배포 업데이트 설치·재시작 | 미검증 | 모의 피드 감지만 검증. 실제 배포 서명·다운로드·설치·재시작 증거가 아님 |

## 발견·수정한 작은 결함

1. **MCP 합산 행 상한 누락.** `max_rows=3`에서 SELECT 두 문장이 6행을 반환했다. `limit_result`가 새 페이지 예산을 적용하기 전에 기존 `bound`를 호출하도록 복구했다. focused 회귀 검사 RED→GREEN과 native 권한 E2E 통과를 확인했다. 캐시에 들어가는 결과에도 기존 실행 상한을 적용한다.
2. **변경 로그 현재 버전 오표시.** `CURRENT_RELEASE`가 0.2.0이었다. 0.3.0으로 수정하고 native 변경 로그와 업데이트 창에서 확인했다.
3. **native 드라이버 로고 누락.** Vite가 작은 WebP를 data URL로 넣었지만 CSP는 self만 허용했다. `assetsInlineLimit: 0`으로 파일을 배포해 CSP를 유지했다. 최종 앱의 로고 이미지 12개가 data URL 없이 정상 디코딩됨을 확인했다.

공유 계약·커맨드 이름·Capabilities는 변경하지 않았다. 숨김 Mongo CPU 증가의 원인은 이번 검증에서 확정하지 않았다. 주기적인 SDK 작업 여부를 별도 분석해야 하며, 재측정으로 첫 관측을 덮지 않았다.

## 검증 명령과 원자료

| 검사 | 최종 결과 | 증거 |
|---|---|---|
| `cargo build --workspace --bins` | 통과 | `target-v1`, 이후 해당 native 앱을 다시 빌드 |
| `cargo test --workspace` | 303 통과, 7 ignored | [Rust 로그](verification/v1-native-030/cargo-test.txt) |
| `cargo clippy --workspace --all-targets -- -D warnings` | 통과 | [clippy 로그](verification/v1-native-030/clippy.txt) |
| `pnpm --dir ui exec tsc --noEmit` | 통과 | exit 0, [영수증](verification/v1-native-030/receipts.json) |
| `pnpm --dir ui test` | 146 통과, 1 skipped | [UI 로그](verification/v1-native-030/ui-test.txt) |
| 실제 앱 E2E | 21 + 5 + 13 통과 | [브리지](verification/v1-native-030/e2e-bridge.txt), [미리보기](verification/v1-native-030/preview-e2e.txt), [권한](verification/v1-native-030/permissions-e2e.txt) |
| 행 상한 회귀 | RED→GREEN | [RED](verification/v1-native-030/row-cap-red.txt), [GREEN](verification/v1-native-030/row-cap-green.txt) |

최초 전체 UI 실행에서는 탐색기 테스트가 5초 제한으로 실패했다(145 통과·1 실패·1 skipped). 단독 실행은 10/10 통과했고, 이후 실제 UI 수정 뒤 최종 전체 실행은 146/146 통과했다. 타임아웃을 늘리거나 테스트를 완화하지 않았다. [최초 실패](verification/v1-native-030/initial-ui-failure.txt)를 보존했다. 최초 Rust 실행의 암호화 SSH 키 실패는 fixture secret store·전용 ssh-agent 누락 때문이었다. 테스트 픽스처를 갖춘 최종 전체 실행이 통과했으며 [최초 실패](verification/v1-native-030/initial-network-fixture-failure.txt)도 보존했다.

7 ignored에는 실제 AI 호출·OS 키체인, 수동 성능 측정, 별도 설치 하네스가 포함된다. AI 출력의 직접 실행 금지 검사는 통과했지만 실제 AI 공급자·OAuth 증거는 없다. native 명령/DOM 계측과 OS 대화상자를 혼용했으며, 전체 과정을 사람의 마우스 입력만으로 수행했다고 주장하지 않는다.

CPU는 macOS `ps`의 누적 CPU 시간(0.01초 해상도) 차이를 약 15초간 관측했다. 정확한 앱 PID, 직접 플러그인 자식, 실행 전후 새 WebKit PID를 집계했고 각 WebKit 역할은 하나였다. 영구적인 CPU 무사용을 보증하는 측정은 아니다. 표시/숨김·원시 PID별 값은 [영수증](verification/v1-native-030/receipts.json)에 보존했다.

## 주요 화면

- 드라이버: [Midnight](design-shots/v1-native-drivers-midnight.png), [Daylight](design-shots/v1-native-drivers-daylight.png)
- PG 표·ERD: [표](design-shots/v1-native-pg-table-midnight.png), [ERD](design-shots/v1-native-pg-erd-daylight.png), [참조 이동](design-shots/v1-native-pg-follow-reference-daylight.png)
- 실행 계획: [PG](design-shots/v1-native-pg-plan-midnight.png), [MySQL](design-shots/v1-native-mysql-plan-daylight.png)
- 뷰어: [그래프](design-shots/v1-native-neo4j-graph-midnight.png), [문서](design-shots/v1-native-mongo-documents-daylight.png), [키-값](design-shots/v1-native-redis-keyvalue-midnight.png)
- 관리: [가져오기 미리보기](design-shots/v1-native-import-preview-daylight.png), [스키마 diff](design-shots/v1-native-schema-diff-midnight.png), [권한](design-shots/v1-native-roles-daylight.png), [백업](design-shots/v1-native-backup-midnight.png)
- 앱: [SSH 지문](design-shots/v1-native-ssh-host-key-daylight.png), [MCP](design-shots/v1-native-mcp-clients-midnight.png), [변경 로그](design-shots/v1-native-changelog-daylight.png), [모의 업데이트](design-shots/v1-native-update-mock-midnight.png)

검증 후 전용 debug 앱·로컬 계측 서버·모의 피드를 종료하고 임시 credential store를 삭제했다. 직접 만든 PG 원본/복원 DB 두 개와 역할도 제거했다. 설치된 사용자 앱과 공유 빌드 산출물은 그대로이며, 릴리스 산출물에는 이 소스 수정 3건을 반영한 별도 재빌드가 필요하다.
