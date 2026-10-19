# AxDB 문서 색인

제품 소개와 설치는 [루트 README](../README.md)에 있다. 아래는 기준 문서와 기록의 위치다. 작업 보고서(`*_REPORT.md` 등)는 당시의 기록이므로 내용을 고치지 않는다. 현재 상태는 기준 문서를 따른다.

## 기준 문서

| 문서 | 내용 |
|---|---|
| [STATUS.md](STATUS.md) | 0.2.0 기준 구현 상태와 남은 한계 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 드라이버 계약, 호스트 명령, 드라이버 프로세스·패키지·설치, 앱 업데이트 |
| [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md) | 드라이버 만들기: crate → SDK `serve` → `cargo xtask package-drivers` → 서명 → 설치, protocol 1 |
| [AGENT.md](AGENT.md) | 에이전트 도구, WebMCP, localhost MCP 브리지, 권한 |
| [DESIGN.md](DESIGN.md) | 디자인 시스템(토큰, 테마, 마이크로 인터랙션) |
| [VERIFICATION.md](VERIFICATION.md) | 검증 결과, 성능 측정, 남은 한계 |
| [RESOURCE_OPT.md](RESOURCE_OPT.md) | 메모리·CPU 측정과 최적화 |
| [RELEASE_SIGNING.md](RELEASE_SIGNING.md) | Apple 서명·공증, 앱 업데이트 서명 키, `scripts/release.sh` |
| [release-notes/](release-notes/) | 버전별 릴리스 노트(`scripts/release.sh`가 읽는다) |

## 설계 결정(ADR)

| 문서 | 결정 |
|---|---|
| [0001](adr/0001-independent-implementation.md) | 독립 설계: Tauri 2 + Rust, 다형 결과 계약, 드라이버 정의 탐색 트리 |
| [0002](adr/0002-agent-interface-webmcp.md) | 에이전트 인터페이스(WebMCP와 localhost MCP 브리지) |
| [0003](adr/0003-installable-drivers.md) | 설치형 드라이버: 외부 프로세스, `.axdriver` 패키지, 서명, 레지스트리 |

## 작업 보고서(기록)

| 영역 | 보고서 |
|---|---|
| 드라이버 | [W1 백엔드](W1-backend-report.md) · [W2 Neo4j](W2-neo4j-report.md) · [W5 SQLite](W5_SQLITE_REPORT.md) · [M1 MongoDB](M1_MONGODB_REPORT.md) · [M2 Mongo ERD·TLS](M2_REPORT.md) · [R1 Redis](R1_REDIS_REPORT.md) |
| 설치형 드라이버 | [P1 플러그인 호스트](P1_PLUGIN_REPORT.md) · [P2 패키지·설치](P2_PACKAGE_REPORT.md) · [P3 드라이버 화면](P3_DRIVERS_UX_REPORT.md) · [P3B 디자인](P3B_DRIVERS_DESIGN_REPORT.md) |
| 에이전트·권한·AI | [W3 에이전트](W3-agent-report.md) · [W7b 브리지](W7b_BRIDGE_READY_REPORT.md) · [A1 권한](A1_PERMISSIONS_REPORT.md) · [AI1](AI1_REPORT.md) |
| 미리보기·편집 | [C1 미리보기](C1_PREVIEW_REPORT.md) · [C2 편집](C2_EDIT_REPORT.md) · [C2 UI](C2_EDIT_REPORT_UI.md) · [C2 네이티브 E2E](C2_NATIVE_E2E.md) |
| 뷰어·내보내기 | [V1 뷰어](V1_VIEWERS_REPORT.md) · [E1 내보내기](E1_EXPORT_REPORT.md) · [뷰어 성능](../bench/VIEWER_PERF.md) |
| 디자인·UI | [D1](D1_DESIGN_REPORT.md) · [D2](D2_MICRO_REPORT.md) · [D3](D3_COMPONENTS_REPORT.md) · [U1 크롬](U1_CHROME_REPORT.md) · [U1 폼](U1_FORMS_REPORT.md) · [U1 뷰](U1_VIEWS_REPORT.md) · [U2 CallChip](U2_CALLCHIP_REPORT.md) · [U2 GlideSelect](U2_GLIDESELECT_REPORT.md) · [U2 BranchedMenu](U2_BRANCHEDMENU_REPORT.md) · [U2 잔여](U2_REMAINING_REPORT.md) · [U3 문구](U3_COPY_REPORT.md) · [U4 상태바](U4_CHROME_REPORT.md) · [U4 사이드바](U4_SIDEBAR_REPORT.md) · [U5](U5_REPORT.md) · [S1 사이드바](SIDEBAR_S1.md) |
| UI 감사 | [UI_AUDIT_NATIVE](UI_AUDIT_NATIVE.md) · [U1 재감사](U1_NATIVE_REAUDIT.md) · [최종 감사](FINAL_UI_AUDIT.md) · [최종 감사 2](FINAL_UI_AUDIT_2.md) · [최종 감사 3](FINAL_UI_AUDIT_3.md) |
| 배포·업데이트 | [U1 업데이터](U1_UPDATER_REPORT.md) · [W6 이름 변경](w6-rename-report.md) · [W7 수정](W7_FIXES_REPORT.md) |

## 작업 명세와 자료

- [tasks/](tasks/): 워커에게 준 작업 명세(기록)
- [design-shots/](design-shots/): 화면 캡처와 측정 원자료(`drivers-*`, `onboarding-*`, `update-*`, `sidebar-*`, `export-*` 등)
- [assets/](assets/): README 배너와 화면 이미지
- [contract-examples.json](contract-examples.json): 계약 예제
