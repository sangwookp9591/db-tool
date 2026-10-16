# 참고: Integration Beam (사용자 제공, React 단일 파일)

구성
- 가운데 제품 타일: 80px, radius 24, 진한 hub와 inset 하이라이트. 아래에 상태 칩("Syncing…", "N connected", "All paused")이 있고, 텍스트 폭에 맞춰 width가 320ms 전이로 움직인다.
- 양쪽 도구 타일: 60px, radius 18, 카드 그림자. 왼쪽 3개는 보내는 쪽, 오른쪽 3개는 받는 쪽이다.
- 타일 사이: SVG cubic S-curve(가로 탄젠트, 좁은 폭에서는 세로) 1.5px 회색 선

빔과 펄스
- 실제 타일의 offset 중심을 재서 곡선을 만든다. ResizeObserver로 다시 재고, 레이아웃이 바뀌면 FLIP으로 타일이 미끄러지며 곡선이 보간된다(MORPH 460ms, easeOutQuint `cubic-bezier(0.22,1,0.36,1)`).
- 펄스: stroke-dasharray 한 구간(길이 min(84, L*0.42))이 easeInOut으로 경로를 따라 이동한다. linearGradient(tail 투명 → mid accent → head 밝은색)를 dash 앞뒤 좌표로 매 프레임 갱신한다. 코어 2px 위에 blur glow 6px(opacity .45)를 겹친다.
- 도착하면 대상 타일에 ring 애니메이션이 1회 돈다(WAAPI, 760ms, 0.96 → 1 → 1.14 scale + opacity).
- 원본 사이클: 왼쪽 펄스가 280ms 간격으로 출발하고, 첫 도착(1500ms) 때 오른쪽 웨이브가 나가며, 1500ms 쉰 뒤 반복한다. IntersectionObserver로 화면에 보일 때만 돈다.

상호작용
- hover와 focus: 해당 빔을 강조(line-hi)하고 나머지는 opacity .35, 툴팁은 중앙 방향으로 뜬다.
- click: 해당 도구를 일시정지한다(점선, 회색 타일, aria-pressed). aria-live로 "N of M connected"를 알린다.
- prefers-reduced-motion이면 펄스와 전이를 끈다. 다크 토큰 세트가 있다.

## AxDB 적용 규칙(코디네이터)
- **연속 루프는 금지다**(유휴 애니메이션 0 규칙). 펄스는 **실제 이벤트가 있을 때만** 해당 빔으로 흐른다.
  - 쿼리 실행 시작: DB 타일 → AxDB 방향 펄스(결과 도착 시 AxDB ring)
  - MCP 도구 호출: 에이전트 타일 ↔ AxDB 방향 펄스(호출 시작 시 AxDB 쪽으로, 응답 시 에이전트 쪽으로)
  - 에이전트 쓰기 승인 대기: 해당 빔이 warning 색으로 1회 펄스하고 점선을 유지한다
- 왼쪽: 연결 프로필(패러다임 아이콘, 상태: 연결/끊김). 오른쪽: 에이전트 클라이언트(local, 범위 클라이언트들. 권한 none이면 점선).
- 클릭하면 일시정지 대신 해당 연결 또는 클라이언트의 상세나 권한 편집을 연다(위험한 토글을 숨겨 두지 않는다).
- 이벤트가 없을 때 rAF는 0이다. 펄스가 끝나면 즉시 정지한다.
- 색은 DESIGN.md 토큰을 쓴다(accent-primary 펄스, accent-secondary head, success와 warning 상태).
