# AxDB 디자인 시스템 (v3)

AxDB는 "가볍고 빠른 다중 DB 작업대"다. 화면은 개발자 도구답게 조밀하고 조용해야 한다. 동작이 생길 때만 반응하고, 상태는 색과 형태로 정확히 구분한다. 장식이 데이터보다 앞서지 않는다.

출처 기록
- 토큰 이름, 의미 규칙, 레시피 구조: **Tabularis 0.28.0 `DESIGN.md`와 테마 체계(Apache-2.0)**를 가져와 고쳐 썼다. 값과 컴포넌트는 AxDB 브랜드에 맞게 바꿨다. 고지는 `THIRD_PARTY_NOTICES.md`에 둔다.
- 마이크로 인터랙션 9종: **React Bits(MIT + Commons Clause)**의 JellyRadio, SwipeRow, CometDial, WakeSlider, CodeSlots, LatticeLoader, BorderGlow, SpringCheck, FuseButton 동작을 참고해 Preact와 CSS로 이식했다. 컴포넌트를 따로 재배포하지 않는다. 고지는 같은 파일에 둔다.

## 1. 원칙

1. **데이터가 주인공이다.** 크롬(UI 틀)은 회색 계열과 hairline 경계로 물러서고, 색은 의미(상태, 타입, 패러다임)에만 쓴다.
2. **조용한 기본, 반응하는 상호작용.** 유휴 상태에서는 어떤 애니메이션도 돌지 않는다. 포인터, 키보드, 데이터 변화가 있을 때만 움직인다.
3. **한눈에 구분되는 상태.** 연결됨, 실행 중, 읽기 전용, 미리보기, 오류, 에이전트 실행을 각각 다른 토큰으로 표시한다. 같은 토큰의 명도만 바꿔 구분하지 않는다.
4. **키보드 우선.** 모든 기능은 키보드로 닿아야 하고, focus는 항상 보인다.

## 2. 토큰

이름은 Tabularis 규약을 따른다. 컴포넌트는 토큰만 쓰고, hex 리터럴은 `ui/src/theme.css`에만 둔다.

| 의도 | 변수 | Midnight(다크, 기본) | Daylight(라이트) |
|---|---|---|---|
| 창 배경 | `--bg-base` | `#0B0D12` | `#F6F7FA` |
| 패널(사이드바, 결과) | `--bg-elevated` | `#11141B` | `#FFFFFF` |
| 팝오버, 모달 | `--bg-overlay` | `#171B24` | `#FFFFFF` |
| 입력 | `--bg-input` | `#0E1117` | `#FFFFFF` |
| 카드, 행 | `--surface-primary` | `#141822` | `#FBFBFD` |
| 칩, 보조 면 | `--surface-secondary` | `#1A1F2B` | `#F0F2F6` |
| hover | `--surface-hover` | `#1D2330` | `#EEF1F6` |
| 선택 | `--surface-active` | `#232B3B` | `#E4E9F3` |
| 본문 | `--text-primary` | `#E7EAF0` | `#151922` |
| 보조 | `--text-secondary` | `#A3ABBA` | `#4A5263` |
| 힌트 | `--text-muted` | `#6B7385` | `#7A8294` |
| 비활성 | `--text-disabled` | `#4A5160` | `#A9B0BD` |
| 강조 글자 | `--text-accent` | `#A89BFF` | `#5B45E0` |
| 강조 채움 위 글자 | `--text-inverse` | `#0B0D12` | `#FFFFFF` |
| 브랜드 강조 | `--accent-primary` | `#8B7BFF` | `#5B45E0` |
| 브랜드 보조(그라디언트 끝) | `--accent-secondary` | `#22D3EE` | `#0891B2` |
| 성공·연결·실행 | `--accent-success` | `#34D399` | `#059669` |
| 경고·미리보기 | `--accent-warning` | `#FBBF24` | `#B45309` |
| 오류·삭제 | `--accent-error` | `#F87171` | `#DC2626` |
| 정보·에이전트 | `--accent-info` | `#60A5FA` | `#2563EB` |
| 경계 | `--border-subtle/default/strong` | `#1C212C / #262C39 / #343B4B` | `#E6E9EF / #D9DEE7 / #C3CAD6` |
| focus | `--border-focus` | `#A89BFF` | `#5B45E0` |

- **브랜드 그라디언트** `--brand-gradient: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary))`. 쓰는 곳은 로고, Run 버튼, 활성 탭 상단 2px 선, BorderGlow 세 곳뿐이다.
- **데이터 타입 색**(`--semantic-string/number/boolean/date/json/binary/null`)
  - 결과 그리드 값과 헤더 타입 배지에 쓴다.
  - NULL은 `--text-muted` 기울임 pill이다. 빈 문자열 `""`과 구분돼야 한다.
- **패러다임 색**(`--paradigm-relational/graph/document/keyvalue`): 파랑, 보라분홍, 초록, 호박색이다.
  - 연결 타일 아이콘과 탭 색 띠에 쓴다.
  - 값은 드라이버 매니페스트의 `paradigm` 계약 필드에서 고른다. **드라이버 id로 분기하지 않는다.** UI driver-id grep 0을 유지한다.
- **반경**: `--radius-sm 4px`, `--radius 6px`, `--radius-lg 10px`, `--radius-xl 14px`
- **그림자**: 다크에서는 그림자 대신 1px 상단 하이라이트(`inset 0 1px 0 #ffffff08`)와 경계를 쓴다. 라이트에서는 `0 1px 2px #0f172a0d, 0 8px 24px #0f172a0f`를 쓴다.
- **간격**: 4px 격자. 툴바 높이 36, 탭 32, 상태바 24, 사이드바 행 28, 결과 행 26.

### 테마
- 기본은 시스템을 따르고, 설정 메뉴에서 Midnight, Daylight, System 중 고를 수 있다.
- 선택은 localStorage에 둔다(뷰어 편의 설정이므로).
- 시작할 때 첫 페인트 전에 적용해 깜박임을 없앤다.
- CodeMirror 테마와 그래프 캔버스 색은 같은 토큰에서 읽는다. 캔버스는 `getComputedStyle`로 한 번 읽고, 테마가 바뀔 때만 다시 읽는다.

## 3. 타이포그래피

| 용도 | 글꼴 | 크기/굵기 |
|---|---|---|
| UI | **Pretendard Variable**(OFL, 한글·라틴) | 본문 13/400, 레이블 12/500, 섹션 제목 11/600 대문자 + 자간 .06em |
| 코드, 결과 값 | **JetBrains Mono Variable**(OFL) | 12.5/400, 숫자는 `tabular-nums` |
| 제목 | Pretendard | 15/600 |

- 글꼴은 npm 패키지로 **로컬 번들**한다. 원격 로드는 금지한다(오프라인 데스크톱 앱이므로).
- 한글은 dynamic-subset(unicode-range) woff2를 써서 필요한 조각만 로드한다.
- `.app` 크기 증가는 3MiB 이하로 한다.

## 4. 레이아웃

```
┌──┬────────────────┬──────────────────────────────────────────────┐
│R │ Sidebar        │ Tabs  ▔▔(활성: 브랜드 2px선, 패러다임 색 점)    │
│a │ 검색 ⌘K        ├──────────────────────────────────────────────┤
│i │ 연결 카드      │ Editor toolbar: ▶Run ⌘↵ │ Preview │ Cancel │ ⚙옵션 │
│l │  ▸ 탐색 트리   │ CodeMirror (BorderGlow on focus)             │
│  │                ├──────────── splitter ────────────────────────┤
│  │                │ Result header: 제목 · [Table|Graph|JSON|ERD] │
│  │                │ metrics chips(ms·rows)  필터                 │
│  │                │ Virtual grid / Graph / Tree                  │
├──┴────────────────┴──────────────────────────────────────────────┤
│ Status bar: ● e2e-pg  PostgreSQL 16 · 읽기전용 · 9 ms · 1 row · MCP ●│
└──────────────────────────────────────────────────────────────────┘
```

- **Rail(48px, lucide 아이콘):** 연결, 이력, 에이전트(클라이언트·활동), AI 도우미, 설정(테마, Graph node limit 등)이다. 지금 헤더에 흩어진 컨트롤을 이 rail과 설정 팝오버로 옮긴다.
- **사이드바(리사이즈 220~420px)**
  - 연결 카드는 패러다임 색 타일 아이콘(lucide `Database`/`Share2`/`FileJson`/`KeyRound`), 이름, 드라이버 이름, 상태 점으로 구성한다. 연결되면 상태 점이 success 색에 글로우를 띤다.
  - 행동 버튼(Connect, Edit, Test, Delete)은 hover나 focus 때만 나타나는 아이콘 버튼과 `…` 메뉴로 바꾼다. 지금처럼 버튼 4개를 늘어놓지 않는다.
  - 연결 카드 아래에 탐색 트리가 펼쳐진다.
- **탭:** 32px 높이에 왼쪽에 패러다임 색 점을 두고, 닫기 버튼은 hover나 활성 상태일 때만 보인다. 수정됨, 실행 중, 읽기 전용 표시를 붙인다.
- **편집기 툴바**
  - Run은 브랜드 그라디언트 채움 버튼에 `⌘↵` kbd 칩을 단다.
  - Preview는 warning 톤 soft 버튼이다.
  - Timeout과 Row limit은 "실행 옵션" 팝오버(⚙)로 옮긴다(CometDial, WakeSlider).
  - 현재 값은 툴바에 작은 칩으로 표시한다(`30s · 1,000 rows`).
- **결과 헤더:** 뷰 전환은 JellyRadio이고, 시간과 행 수 메트릭은 숫자가 바뀔 때 CodeSlots식 숫자 스프링을 쓴다.
- **상태바:** 활성 연결, 서버 버전, 접근 모드, 마지막 실행 시간과 행 수, MCP 브리지 상태를 둔다.
- **이력:** 하단 접이식 패널이나 rail 패널로 둔다. 행은 SwipeRow를 쓴다.
- **빈 상태:** 로고 마크, 한 줄 설명, "연결 추가"와 "샘플 쿼리" 두 동작을 둔다.

## 5. 마이크로 인터랙션 매핑

| React Bits | AxDB 위치 | 동작 규칙 |
|---|---|---|
| JellyRadio | 결과 뷰 전환(Table/Graph/JSON/ERD), AI effort와 인증 방식 | 선택한 칩이 부풀고 이웃이 밀려난다. 220ms spring이며 transform만 쓴다 |
| SwipeRow | 이력 행 | 트랙패드 가로 스와이프나 드래그로 "새 탭에서 열기, 복사, 삭제"가 드러난다. 같은 동작은 hover 아이콘과 키보드(`→`)로도 할 수 있다 |
| CometDial | 실행 옵션의 Timeout(1s~600s, 로그 눈금) | 드래그·휠·화살표 키로 조작한다. 혜성 꼬리는 조작 중에만 보인다 |
| WakeSlider | Row limit(100~100,000), Graph node limit | 막대형 슬라이더. 드래그 속도만큼 물결이 생기고 멈추면 사라진다. 숫자 입력 칸도 함께 둔다 |
| CodeSlots | 메트릭 숫자(ms, rows), MCP 토큰 표시 | 값이 바뀔 때만 자리별로 스프링한다. 같은 값이면 움직이지 않는다 |
| LatticeLoader | 실행 중 상태(결과 영역 상단 행), AI 응답 대기 | 격자 셀이 물결로 밝아지고 옆에 실시간 스톱워치가 돈다. **실행 중일 때만** 돈다. 끝나면 즉시 정지하고 DOM에서 제거한다 |
| BorderGlow | focus된 편집기 패널, AI 패널, 연결 추가 대화상자 | 포인터 방향을 따라 경계 메시 그라디언트가 밝아진다. pointermove 때만 CSS 변수를 갱신하고, rAF 1회로 묶는다 |
| SpringCheck | 체크박스 전체(읽기 전용, MCP 브리지, 행 포함 등) | 스프링으로 박스가 채워지고 체크가 그려진다. 레이블 취소선은 쓰지 않는다(의미가 다름) |
| FuseButton | 연결 삭제, 이력 삭제, 토큰 폐기 | 누르면 완료 상태가 되고 5초짜리 "실행 취소" 퓨즈가 탄다. 퓨즈가 다 타야 실제로 삭제한다. 그 전에 취소하면 아무 일도 없다 |
| CommandPalette · 21st.dev Command 참고 | 연결·테이블·최근 쿼리 검색 | `⌘K`/`Ctrl+K`, 검색, 화살표·Enter·Escape. 기존 모달이 열려 있으면 새 팔레트를 열지 않는다. 진입 160ms, 닫으면 제거 |
| SlidingIndicator · affectUI Segmented Control 참고 | 탭과 세그먼트 | 기존 tab/radio 의미를 보존하며 선택 표시줄만 220ms 동안 translate·scale한다. 값이 같으면 정지 |
| SpotlightCard · React Bits | 사이드바 연결 카드 | 포인터가 움직일 때만 rAF 1회로 묶어 강조 면을 translate한다. 키보드 focus는 정적 경계로 표시 |
| ToastStack · affectUI Toaster 참고 | 실행 완료·오류·복사 알림 | 160ms 진입 후 정지. status/alert로 알리고 닫기 버튼으로 제거한다. 자동으로 사라져 읽기를 방해하지 않는다 |
| Tooltip · 21st.dev Tooltip 참고 | 아이콘 버튼과 실행 단축키 도움말 | hover·focus로 열고 Escape로 닫는다. 트리거의 이름을 보존하고 aria-describedby로 설명을 연결한다. 120ms 진입 후 정지 |
| Skeleton · affectUI Shimmer Loader 참고 | 결과 헤더와 탐색 패널의 로딩 자리 | 160ms 진입 후 정적인 자리 표시. running=false면 제거한다. 그리드 셀·캔버스 내부나 무한 shimmer에는 적용하지 않는다 |
| IntegrationBeam · 사용자 제공 레퍼런스 | 연결 프로필 ↔ AxDB ↔ 에이전트 활동 | 실제 이벤트에서 `pulse(fromId,toId,tone)`을 호출할 때만 650ms 빔과 760ms 도착 ring을 재생한다. 승인 대기는 warning 점선, 타일 클릭은 상세 열기. 유휴 rAF·자동 반복 없음 |

추가 7종의 원본 링크와 선택 이유, 재사용 API 및 검증 범위는 [D2_MICRO_REPORT.md](D2_MICRO_REPORT.md)에 기록한다. 재사용 컴포넌트의 갤러리 검증과 실제 앱 호출부 연결은 별도 증거로 확인한다.

### 사용자 추가 레퍼런스 2차

| 컴포넌트 | AxDB 위치 | 동작·근거 |
|---|---|---|
| Button | 모든 버튼 | 기존 aria/testid/ref 유지. 그리드·트리는 bare 경로로 단일 native DOM과 무동작을 유지한다. Run은 brand, 실행·테스트·AI·저장 대기에만 loading을 켠다 |
| SuggestionCard | AI 응답의 코드 블록과 대안 | 수락은 새 탭 삽입만 한다. 삽입 취소는 탭을 닫고 수정·실행 이력이 있으면 확인한다. 검사 수준은 실제 로컬 파서·조회 스키마·사용자 요청 READ ONLY EXPLAIN 근거로만 표시한다 |
| ToolCall / Group | MCP 활동과 AI 응답 맥락 | 실제 호출 메타데이터·수집 시간·기존 redaction을 거친 맥락만 표시한다. 미기록 시간은 생략하고 쓰기·미리보기·권한 거부에 Retry를 연결하지 않는다 |

출처는 [사용자 제공 2차 명세](design-shots/ref-components-batch2.md)와 [D3 보고서](D3_COMPONENTS_REPORT.md)다.

### 모션 규칙(필수)
- 애니메이션 라이브러리를 추가하지 않는다(motion, gsap, framer 금지). CSS transition, keyframes, WAAPI와 작은 spring 함수 하나만 쓴다.
- 기본적으로 transform과 opacity만 애니메이션한다. 사용자 승인 예외로 작은 Button의 라벨 변경 시 width를 480ms 동안 1회 보간할 수 있다. 그리드·캔버스 내부에서는 이 예외를 쓰지 않는다.
- 유휴 시 진행 중인 애니메이션은 0개다. 반복 애니메이션은 LatticeLoader 실행 중, Button loading, ToolCall running에만 존재하며 끝나면 즉시 제거한다.
- `prefers-reduced-motion: reduce`이면 모든 spring과 물결을 끄고 즉시 상태로 바꾼다.
- 결과 그리드와 그래프 캔버스 안에는 마이크로 인터랙션을 넣지 않는다(성능 영역).

## 6. 아이콘
- **lucide-preact**(ISC)를 쓰고, 트리 셰이킹으로 쓰는 아이콘만 번들한다. 크기 16, stroke 1.75.
- 이모지와 문자 아이콘(`×`, `▸`, `…`)은 아이콘으로 바꾼다.

## 7. 접근성과 호환성
- **기존 `aria-label`, role, 키보드 단축키를 바꾸지 않는다.** E2E(`bench/e2e-bridge.mjs`, `preview-e2e.mjs`, `permissions-e2e.mjs`)와 에이전트 도구 레지스트리가 이것들에 의존한다. 새 컨트롤에는 새 aria-label을 붙인다.
- 대비: 본문 4.5:1 이상, 보조 텍스트 3:1 이상이다(두 테마 모두).
- focus ring: `0 0 0 2px var(--bg-base), 0 0 0 4px var(--border-focus)`

## 8. 성능 예산(회귀 금지)
- W4 지표: 10만 행 첫 표시 ≤ 50ms, 흔들림 0, CLS 0, 그래프 300노드 팬·줌 p95 ≤ 17ms, 유휴 CPU ≈ 0%(60초, 0.1% 미만)
- 첫 페인트까지 시간과 `.app` 크기 증가는 측정해 보고한다. 글꼴 때문에 늘어나는 크기는 3MiB 이하다.
- JS 번들 증가는 gzip 기준 40KB 이하다(아이콘 포함).
