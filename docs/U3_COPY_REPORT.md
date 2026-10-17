# U3 한국어 UI 문구 통일 보고서

기준: `b92d752` 대비 작업 트리. 결정: UI 문구는 한국어로 통일하고, 기술 용어·제품명·DB 원본 오류·쿼리 텍스트는 유지한다(메모리 `ui-language-policy`). aria-label·role·data-testid·단축키는 바꾸지 않았다(DESIGN.md §152). 이전 부분 작업(Luna)의 변경을 유지하고 확장했다.

## 변경 요약

### 드라이버·호스트(Rust)

- **연결 필드 라벨(매니페스트)**: 사용자·비밀번호·데이터베이스·인증 데이터베이스·TLS 모드·TLS CA 파일·데이터베이스 파일·읽기 전용으로 열기, Redis 5개 필드(`Host`·`Port`는 유지, `비밀번호`·`데이터베이스 번호`·`사용자`). 키·값·enum은 그대로다. 매니페스트에는 옵션 설명 필드가 없어 TLS 옵션 값에 붙일 한국어 힌트가 없다(값만 표시).
- **쿼리 언어 표시명·탐색기 액션 라벨**: `Redis 명령`, `데이터 보기`, `쿼리 열기`, `스키마 그래프`, `값 보기 (TYPE / TTL)`, `값 가져오기`(Neo4j는 `explorer.rs`).
- **미리보기·편집 노트·경고(앱이 만드는 문장)**: `All data and DDL changes were rolled back` 등 14종을 Rust에서 한국어로 바꿨다(`dbcore/preview.rs`, `driver-postgres|sqlite|neo4j`의 `preview.rs`·`edits.rs`). `ChangePreview.notes`/`warnings`를 읽는 UI·MCP 클라이언트 모두 한국어를 받는다. UI 쪽 번역표는 두지 않았다(중복 방지).
- **앱이 만드는 오류 메시지**: 미리보기·편집·승인·권한·세션·프로필·AI 설정/로그인 흐름의 사용자 행동 가능한 메시지 약 75종을 한국어로 바꿨다(`Preview required`→`미리보기가 필요합니다`, `In-app approval required`→`앱에서 승인해야 합니다`, `Execution cancelled`→`실행이 취소되었습니다` 등). `ErrorKind` 값과 DB가 돌려준 텍스트는 그대로다.
- Rust 단언 2곳(`src-tauri/src/edits.rs`의 `"Preview required"`는 같은 문자열 치환으로 함께 변경, `driver-sqlite/tests/sqlite.rs`의 `"user transaction"`→`"사용자 트랜잭션"`)을 갱신했다.

### UI

- 미리보기 제목 `수정 · 영향 2행`(그래프 쓰기는 `건`), `변경 전(샘플)`, 오류 종류 배지(`Query`→`쿼리` 등), 권한 결정(`allowed`→`허용`)은 `ui/src/previewCopy.ts`가 표시 시점에만 바꾼다(+`previewCopy.test.ts`). 모르는 값은 원문 그대로 보인다.
- 상태 표시줄 `12 ms · 24행`, 이력(`UI`/`에이전트`), 탭 제목(`쿼리 N`, `다이어그램 · …`), 명령 팔레트(`연결 추가`·`연결`·`최근 · …`), 그래프 속성(`레이블`·`유형`·`시작 → 끝`·`이웃 노드 확장`), 에이전트 패널(`MCP 활동`, `연결된 데이터베이스 N개`, `승인 대기 중`), 입력 검증·편집·파라미터 오류, 파라미터 영역, 슬라이더 보이는 라벨, 모의 모드 문구.
- **숫자 표기**: 결과 막대·상태 표시줄·토스트·이력·그래프·에이전트·편집 대기 수 등 화면에 나오는 개수·시간을 `toLocaleString()`으로 통일했다.
- **접근성 이름 보존**: 보이는 글자가 곧 접근 가능한 이름이던 곳은 기존 영어 이름을 유지했다.
  - 연결 폼 입력/선택/체크박스: `connectionFieldAriaLabel(field.key)`가 기존 영어 이름을 돌려준다(키 기준, 드라이버 ID 분기 없음). Redis는 기존 `host`/`password`가 `Host`/`Password`로 대소문자만 달라진다.
  - 탭 닫기 버튼/탭 본문: `Tab.aria`에 기존 `Query N`·`Diagram · …`을 보관한다.
  - Integration Beam 노드: `BeamNode.name`(기존 `MCP activity`)을 aria 기준으로 쓴다.
  - 그래프 `Browse label / table`·`Expand neighbors`: 같은 영어 `aria-label`을 새로 붙였다(bench가 이름으로 조회).
  - `aria-label` 리터럴 diff(`b92d752` 대비): 제거·변경 0건, 리터럴 2건 추가. 템플릿 2개(`Close ${tab.aria ?? tab.title}`, `${node.name ?? node.label}: …`)는 표현식만 바뀌고 런타임 값은 같다.

### bench·테스트

- `bench/preview-e2e.mjs`: 거부 확인 정규식 `/refus|reject|not.*roll/i`에 `|거부|롤백`을 더했다. 같은 동작(비롤백 문장이 거부되었는지)을 계속 검증한다.
- `bench/verify-viewer.mjs`: `Table`/`Graph` 글자로 찾던 곳을 `Show … results` aria-label로 바꿨다. 이전 커밋에서 이미 한국어였던 `＋ Query · SQL`·`Save`·`Nodes & relationships…` 조회(`verify-viewer.mjs`, `viewer-workspace.mjs`, `v1-viewers.mjs`)도 현재 이름에 맞췄다. bench는 네이티브 앱·DB가 필요해 이번 환경에서 재실행하지 않았다.
- 테스트: `values.test.ts`·`edits.test.ts`·`params.test.ts` 오류 메시지, `mock.test.ts`(`취소`), `branched-menu.test.ts`(메뉴 항목) 기대값을 한국어로 바꾸고 `previewCopy.test.ts`를 추가했다.

## 용어집 (한 개념 = 한 용어)

| 개념 | 표기 | 개념 | 표기 |
|---|---|---|---|
| Connection | 연결 | Database | 데이터베이스 |
| User | 사용자 | Password | 비밀번호 |
| Authentication | 인증 | Read-only | 읽기 전용 |
| History | 이력 | Preview | 미리보기 |
| Rows (count) | `N행` | Affected | `영향 N행`(그래프 쓰기는 `N건`) |
| Table / Graph view | 표 / 그래프 (`JSON`은 유지) | Message view | 메시지 |
| Browse (rows) | 데이터 보기 | Open query | 쿼리 열기 |
| Before/After sample | 변경 전/후(샘플) | Rolled back | 롤백했습니다 |
| Cancelled / Timed out | 취소되었습니다 / 시간이 초과되었습니다 | Statement kind | 추가·수정·삭제·DDL·그래프 쓰기·기타 |
| Error kind | 연결·인증·쿼리·시간 초과·취소됨·미지원·내부·충돌 | Decision | 허용·거부·승인 |
| Agent | 에이전트 | Limit | 한도 |
| Mock | 모의(데모) | | |

## 검증

| 항목 | 결과 |
|---|---|
| `pnpm --dir ui exec tsc --noEmit` | 통과 |
| `pnpm --dir ui test` | 통과 — 18 파일 / 88 테스트(브라우저 mock 기반 `branched-menu.test.ts` 포함) |
| `pnpm --dir ui build` | 통과 |
| `cargo test --workspace` (`CARGO_TARGET_DIR=target-u3`, PG·Neo4j 환경 변수 지정) | 통과 — 26개 결과 모두 ok, 실패 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 통과 |
| `git diff --check` | 통과 |
| aria-label 리터럴 diff | 제거·변경 0 (위 참고) |

### 레이아웃 (브라우저 mock, ego-browser)

- 상태: 연결 대화상자(고급 설정 펼침), 결과, 변경 미리보기, 이력 패널, 에이전트 클라이언트 대화상자 × 960×600·1280×800 × 다크·라이트 = 20 조합.
- 같은 스크립트로 `b92d752`(기준선)와 현재를 모두 실행하고 `overlap-probe.js` 결과를 대조했다. `text-clipping`·`overlap`·`viewport-overflow`·`focus-ring-clipped`는 20개 조합 모두 기준선과 같고 새 항목이 없다. 새로 보인 `occlusion` 4건은 같은 요소가 모달/이력 패널에 가려지는 기존 항목이며 가리는 요소만 다르다(가상 스크롤 행 차이 포함).
- 스크린샷 12장(`sips -Z 1200`): `docs/design-shots/u3-{connection,results,preview,history,agent}-{960,1280}-{dark,light}.png` 중 12개. 이전 부분 작업의 6장은 지우고 다시 찍었다.
- 육안으로 확인한 곳: 탭, 상태 표시줄, 연결 폼 라벨, 미리보기 제목, 이력 항목, 에이전트 빔 라벨(960×600). 잘림은 없다. 스크린샷은 Rust 메시지 번역 전에 찍었으며, 이후 UI 변경은 mock 오류 문구 2개와 번역표 제거뿐이다.

## 체크리스트 처리 결과 (`u3-english-checklist.txt` 75줄)

| 항목(파일:줄) | 처리 | 내용 |
|---|---|---|
| `App.tsx:134,93` 탭 제목 `Diagram · …`/`Query N` | 번역 | `다이어그램 · …`/`쿼리 N`. 접근성 이름은 `Tab.aria`로 유지 |
| `App.tsx:146` 이력 불러오기 안내 | 번역 | `이력을 불러오려면 연결한 뒤 쿼리 탭을 여세요.` |
| `App.tsx:197` `MCP 클라이언트`, `ClientsPanel.tsx:41`, `AgentPanel.tsx:69,70`, `AssistantPanel.tsx:107,108` | 유지 | 이미 한국어이거나 기술 용어·제품명(`MCP`, `API`, `OS`, `Claude`, `ant CLI`, `Anthropic`) |
| `App.tsx:200`, `215`, `222` 라벨 `History N`/`Add connection`/`Session credential` | 유지(aria) | Dialog·SwipeRow의 접근성 이름(`label` prop). 보이는 제목은 이미 한국어(`연결 인증`). 팔레트 항목 `Add connection`은 보이는 라벨을 `연결 추가`로 바꾸고 검색어 `add connection`만 남김 |
| `Connections.tsx:167` `Loading…` | 번역 | `불러오는 중…` |
| `Connections.tsx:12,36,57`, `ClientsPanel.tsx:33`, `AssistantPanel.tsx:16`, `registry.ts:6–15`, `webmcp.ts:4` `Promise`/`Omit` | 보이지 않음 | 타입 선언 |
| `DocumentTree.tsx:16` `Copy path` | 번역 | 스크린리더 전용 글자 `경로 복사`(aria-label은 유지) |
| `EditableTable.tsx:44` 모의 편집 안내 | 번역 | `모의 모드: 편집은 예시이며 …` |
| `GraphView.tsx:206` `Zoom in/out` | 번역 | 툴팁 `확대`/`축소`(aria 유지) |
| `GraphView.tsx:214` `Browse label / table`·`Expand neighbors`·`start → end`, `id` | 번역·유지 | `레이블 / 테이블 탐색`·`이웃 노드 확장`·`시작 → 끝`, `id`는 속성 키(유지). 두 버튼은 기존 영어 `aria-label` 신설 |
| `NavigationTab.tsx:35` | 번역 | `스키마 그래프 불러오기 / 다시 시도` |
| `QueryTab.tsx:121` `Apply row changes` | 유지(aria) | Dialog `label`(접근성 이름); 제목은 `변경 적용` |
| `QueryTab.tsx:88,91` 실행 옵션 툴팁·파라미터 문구 | 번역 | `실행 옵션`, `파라미터 · JSON 객체`, `정상/오류`, 형변환 안내 |
| `ClientsPanel.tsx:38` `MCP activity` | 번역 | 보이는 라벨 `MCP 활동`, 접근성 이름은 `BeamNode.name`으로 유지 |
| `registry.ts` 100·101·20·42·43·60·68·71·77·90·99, `webmcp.ts:30` | 보이지 않음 | MCP 클라이언트(LLM)가 읽는 도구 응답. 지시대로 유지 |
| `mock.ts:125,128` 액션 라벨 | 번역 | `데이터 보기`·`이름 삽입`·`쿼리 열기`·`다이어그램 보기` |
| `mock.ts:4` `localhost` | 유지 | 입력 예시 값 |
| `mock.ts:72,82` 결과 메시지 | 번역 | `3건 영향`, `두 번째 결과`(+제목) |
| `mock.ts:8,9` `TLS 모드`·`TLS CA 파일` | 유지 | 이미 한국어(`TLS`는 기술 용어) |
| `perf/**` 6줄 | 보이지 않음 | 벤치마크 전용. 지시대로 유지 |
| `BranchedMenu.tsx:86` `Branched menu` | 유지(aria) | 내비게이션 랜드마크 접근성 이름 기본값 |
| `CommandPalette.tsx:54` `Esc` | 유지 | 키 이름 |
| `FuseButton.tsx:33` `Delete`, `LatticeLoader.tsx:6` `Running` | 유지(기본값) | 앱 호출부는 모두 한국어 라벨을 넘긴다. 기본값은 개발 갤러리에서만 쓰여 갤러리·테스트가 의존 |

## 남은 영어 문구와 이유

| 문구 | 이유 |
|---|---|
| `Host`, `Port`, `URI`, `TLS`, `SQL`, `Cypher`, `JSON`, `MCP`, `PK`, `NULL`, `ObjectId`, `RFC3339`, 값 형식(`Text`·`Int`…), 도구 이름(`run_query` 등), `UI`(이력 출처), 단위 `ms`·`s` | 기술 용어·식별자·단위 |
| `AxDB`, `PostgreSQL`, `Neo4j`, `MongoDB`, `Redis`, `SQLite`, `Anthropic`, `Claude`, `ant CLI` | 제품명 |
| 쿼리 텍스트, 결과 제목(쿼리 첫 줄), DB가 돌려준 오류·상세, 서버 버전 문자열, Neo4j 카운터 이름 | DB·드라이버 원본 |
| 드라이버 런타임 결과 문구(`MongoDB … completed`, `Redis key`, `MongoDB command`)와 **연결·검증 단계의 앱 생성 오류**(예: `Redis host is required`, `Invalid Redis port`, `Cannot reach app webview`) `DbError` 생성 지점 기준 드라이버 쪽 약 190곳(전체 392곳 중, 일부는 번역한 문구와 겹침) | 이번에는 미리보기·편집·승인·권한·세션·AI 흐름까지만 번역했다. 나머지는 연결 필드 검증 문구가 대부분이고 DB 오류와 섞여 있으며 양이 많다. 후속 작업 대상 |
| 잠금·저장소·키체인·직렬화 등 내부 오류(`Session lock unavailable`, `Cannot read profiles file` …), 벤치 하네스 오류 | 사용자가 조치할 수 없는 내부 오류 |
| 탐색기 노드 라벨 `Tables`·`Views`·`Columns`·`Indexes`·`public`·`Labels`·`Archive` | 노드 라벨이 곧 `aria-label`(`Expand Tables`)이고 Rust 통합 테스트·`bench/w5-sqlite-smoke.mjs`가 이 라벨로 노드를 찾는다 |
| 모의 샘플 데이터: 연결 이름 `Analytics sandbox`·`Graph playground`·`Document studio`, `Person N`, `Alice` 등 | 사용자 데이터에 해당하고 `Connect or disconnect Analytics sandbox` 같은 aria-label·`branched-menu.test.ts`가 의존. 모의 모드는 `데모`·`모의` 표시로 구분 |
| 모의가 흉내 내는 DB 오류 `Read-only transaction rejects writes.`, `Syntax error near "error"`, `Cannot reach host` | DB 원본 오류 모사 |
| `agent/registry.ts`·`agent/webmcp.ts`의 도구 설명·오류, `App.tsx`의 `host.open/openTab` 오류 | 사람이 아니라 MCP 클라이언트(LLM)에게 가는 프로토콜 텍스트 |
| 브라우저 엔진 `JSON.parse` 오류(파라미터 오류 상세), 입력 예시(`2026-12-01T00:00:00Z`, `1,3,5`, `/path/to/database.sqlite`), `brew install anthropic/tap/ant`, 모델 이름 | 엔진 메시지·예시 값·명령어 |
| `ui/gallery.tsx`, `ui/ExtraGallery.tsx`, `perf/**` | 개발 전용(`import.meta.env.DEV`, `?gallery`)·성능 하네스. `gallery-check.mjs`와 테스트가 영어 문구에 의존하며 제품 UI에 포함되지 않는다 |
| 모든 `aria-label`(예: `Run query`, `Result views`) | 규칙상 번역 금지(E2E·에이전트 레지스트리) |

## 한계·후속

- 위 표의 "연결·검증 단계 앱 생성 오류"와 AI 어시스턴트의 일부 내부 오류는 아직 영어다. 필요하면 같은 방식(문자열 치환 + 테스트 갱신)으로 이어서 처리할 수 있다.
- `영향 N행`은 Neo4j에서 그래프 작업 수 합계다. 미리보기 제목은 `GraphWrite`일 때 `건`으로 표시하지만 결과 막대의 `영향 N행`은 문장 종류를 몰라 `행`으로 표시한다.
- 연결 폼 접근성 이름 키 표(`connectionFieldAriaLabel`)에 없는 새 필드 키는 한국어 라벨이 그대로 aria 이름이 된다. 새 컨트롤이므로 규칙상 문제는 없다.
- `docs/DESIGN.md` 용어 표와 `bench`의 네이티브 E2E는 갱신·재실행하지 않았다.
