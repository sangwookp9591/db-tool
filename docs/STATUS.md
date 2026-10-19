# 현재 구현 상태 (0.2.0)

코드와 작업 보고서를 확인해 분류했다. ✅ 완료, 🟡 부분 구현(한계 명시), ❌ 미구현. 수치의 원자료와 한계는 링크한 보고서에 있다. 처음 세운 v2 고도화 계획(Wave 1~3)은 모두 끝났고, 0.2.0에서 드라이버 설치형 구조와 앱 업데이트가 추가됐다.

## 1. 드라이버 구조

| 항목 | 상태 | 근거 |
|---|---|---|
| 드라이버 계약(Driver/Session, Manifest, Capabilities, typed Value, 다형 ResultSet) | ✅ | `crates/dbcore`, [ARCHITECTURE](ARCHITECTURE.md) |
| 설치형 드라이버: 앱은 드라이버 없이 배포, 드라이버는 별도 프로세스 | ✅ | [ADR-0003](adr/0003-installable-drivers.md), `src-tauri/src/plugin_host.rs`, `crates/driver-protocol`, `crates/driver-sdk` |
| `.axdriver` 패키지, Ed25519 서명·SHA-256 검증, 원자적 설치·취소·삭제·업데이트 | ✅ | `crates/driver-package`, `src-tauri/src/driver_install.rs`, [P2 보고서](P2_PACKAGE_REPORT.md) |
| 드라이버 화면, 온보딩의 드라이버 선택, 설치 진행·대기열 | ✅ | [P3 보고서](P3_DRIVERS_UX_REPORT.md) |
| 실제 드라이버 5종(PostgreSQL · Neo4j · SQLite · MongoDB · Redis) | ✅ | 각 `crates/driver-*`. 관계형·그래프·문서·키-값 패러다임을 실제 서버로 검증했다. MongoDB·Redis 추가 때 `ui/`와 `dbcore` 변경은 0 |
| 외부 프로세스 경로의 실DB 계약 테스트 | ✅ | 기존 계약 테스트 원문을 플러그인 경로에서도 실행한다(29/29, [P1 보고서](P1_PLUGIN_REPORT.md)) |
| 드라이버 패키지의 공식 배포(크로스 플랫폼 빌드, 서명, 목록 게시) | 🟡 | `cargo xtask package-drivers`와 `scripts/publish-drivers.sh`가 있다. 실제 게시와 각 OS 설치 검증은 릴리스 작업이다([RELEASE_SIGNING](RELEASE_SIGNING.md)) |

설치형 구조의 비용은 [P1 보고서](P1_PLUGIN_REPORT.md)에 있다. 10만 행 기준 첫 UI 채널 행 +1.01%, 전체 완료 +11.63%(debug 빌드, 예산 +20% 이내)다. 드라이버 자체는 결과를 모아서 반환하므로 DB 커서 수준의 스트리밍은 아니다.

## 2. 뷰어와 연결된 탐색

| 뷰어/흐름 | 상태 | 근거 |
|---|---|---|
| 표(가상화, 정렬·필터, 타입 헤더) | ✅ | VirtualTable, [VIEWER_PERF](../bench/VIEWER_PERF.md) |
| 그래프(Canvas, Worker 레이아웃, 확장, 검색) | ✅ | GraphView, [V1 보고서](V1_VIEWERS_REPORT.md) |
| 문서(가상화 JSON 트리) | ✅ | DocumentTree |
| ERD(PG FK, SQLite, Neo4j, Mongo 샘플 추론 참조) | ✅ | SchemaView, [M2 보고서](M2_REPORT.md). Mongo 관계는 FK가 아니라 샘플(최대 200문서) 추론이다 |
| 키-값(Redis 키 계층, 자료형별 표) | ✅ | [R1 보고서](R1_REDIS_REPORT.md). SCAN 페이지는 스냅숏이 아니다 |
| 뷰어 간 연결 탐색(FK 셀 → 참조 행, 그래프↔표 동기화, 탐색 이력) | ✅ | [V1 보고서](V1_VIEWERS_REPORT.md) |
| 결과 내보내기(CSV·JSON·Markdown·NDJSON, ERD·그래프 이미지) | ✅ | [E1 보고서](E1_EXPORT_REPORT.md) |

## 3. 변경 미리보기와 편집

✅ 쿼리 미리보기(실행 후 반드시 롤백, PG·Neo4j·SQLite)와 표 편집(편집 → 미리보기 → 단일 트랜잭션 적용, PG·SQLite, 기본 키 필요). [C1](C1_PREVIEW_REPORT.md), [C2](C2_EDIT_REPORT.md), [네이티브 E2E](C2_NATIVE_E2E.md). MongoDB·Redis는 미리보기·편집을 지원하지 않는다.

## 4. 권한과 MCP

| 항목 | 상태 |
|---|---|
| 에이전트 실행의 기본 읽기 전용(DB 수준 강제) + 쓰기 승인 | ✅ |
| 연결 단위 읽기 전용(UI 실행에도 호스트가 강제) | ✅ |
| 에이전트·클라이언트별 권한 프로필(연결별 none / read / preview / write-with-approval) | ✅ |
| MCP 브리지: 범위 토큰(해시 저장), 허용 도구, 감사 로그, 회전·폐기 | ✅ |

근거: [A1 보고서](A1_PERMISSIONS_REPORT.md), [AGENT.md](AGENT.md). Redis의 읽기 전용은 드라이버 허용 목록이며 서버 ACL 수준이 아니다. MCP 도구 `list_drivers`는 설치된 드라이버만 반환한다.

## 5. AI 지원

✅ 스키마와 작업 맥락을 제공하는 MCP 도구, 앱 안의 AI 도우미(기본 꺼짐, 쿼리는 새 탭에 제안만 하고 실행하지 않음, Claude 로그인 또는 API 키). Claude 로그인 경로는 실제 호출로 확인했고 API 키 경로는 mock으로만 검증했다. [AI1 보고서](AI1_REPORT.md), [VERIFICATION](VERIFICATION.md) 6절.

## 6. 앱 배포와 업데이트

| 항목 | 상태 | 근거 |
|---|---|---|
| 첫 실행 온보딩(7장면) | ✅ | `ui/src/onboarding`, [온보딩 검증](../ui/src/onboarding/VERIFICATION.md) |
| 앱 자동 업데이트(서명된 `latest.json`, 릴리스 노트 대화상자, 진행률, 다시 시작) | ✅ 구현 | [U1 업데이터 보고서](U1_UPDATER_REPORT.md). 실제 서명 업데이트를 이전 버전 앱이 받아 설치하는 end-to-end 검증은 배포 환경에서 해야 한다 |
| 릴리스 스크립트(`scripts/release.sh`: 서명·공증·staple, Windows NSIS, `latest.json`) | ✅ | [RELEASE_SIGNING](RELEASE_SIGNING.md). 가짜 도구로 흐름을 검증했고 실제 Apple·GitHub 호출은 릴리스 때 수행한다 |
| macOS universal dmg, Windows x64 exe | ✅ 빌드 절차 | Windows 설치 파일에는 코드 서명이 없어 SmartScreen 경고가 뜬다. Linux 앱 설치 파일은 배포하지 않는다 |

## 7. 성능·보안·리소스

| 항목 | 상태 |
|---|---|
| 뷰어 성능(Chromium 기준) | ✅ 300노드 그래프 팬·줌 p95 16.7ms 등. 10만 행 cold 첫 표시 50ms 목표는 미달(83.0ms). [D1](D1_DESIGN_REPORT.md), [VIEWER_PERF](../bench/VIEWER_PERF.md) |
| 호스트 메모리 반납(mimalloc) | ✅ 같은 쿼리 50회 뒤 host 696 → 63MiB. [RESOURCE_OPT](RESOURCE_OPT.md) |
| 드라이버 프로세스 메모리 | 🟡 유휴 RSS 6~17MiB를 측정했다. 대량 결과 뒤 잔류 메모리는 [P1 보고서](P1_PLUGIN_REPORT.md)가 후속 과제로 남겼다 |
| TLS(PG·Neo4j·Mongo 단일 서버) | ✅ Neo4j 클러스터 routing TLS는 미검증 |
| 키체인 저장, 비밀의 로그·프로필 미노출 | ✅ |

## 8. 남은 한계

- Windows와 Linux는 앱 GUI를 실제 기기에서 검증하지 않았다(Linux는 Docker에서 드라이버와 실DB 테스트만, Windows는 크로스 빌드와 Wine 부분 실행). [VERIFICATION](VERIFICATION.md) 6절.
- Redis: TLS, Cluster/Sentinel, 상태를 유지하는 트랜잭션이 없다.
- 남은 UI 한계(minor)는 [VERIFICATION](VERIFICATION.md) 8절과 [최종 감사](FINAL_UI_AUDIT_3.md)에 있다.
- `ui` 테스트 중 일부는 CPU 경쟁 환경에서 시간 제한에 민감하다([U1 업데이터 보고서](U1_UPDATER_REPORT.md)).
