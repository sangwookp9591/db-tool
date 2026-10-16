# 사용자 제공 레퍼런스 2차 (React 단일 파일 3종)

출처: 사용자가 대화에 붙여 넣은 React 컴포넌트. 원본 저작자 표기가 없으므로 "사용자 제공 레퍼런스"로 주석에 남기고, 동작을 Preact와 AxDB 토큰으로 다시 구현한다.

## A. Smart Recommendation → AI 쿼리 제안 카드
- 원본 구조
  - 회색 tray(padding 6, radius 22) 안에 흰 카드(radius 16)가 있다.
  - 헤더 버튼(스파크 아이콘, 제목, 대안 선택 시 "Alternative" pill, chevron 회전)을 누르면 본문이 grid-template-rows 0fr→1fr(340ms)로 접히고 펼쳐진다. reason이 바뀌면 swap 애니메이션이 1회 돈다.
  - footer: 신호 막대 3개(높이 6/9/12px, 레벨 1 호박·2 accent·3 success), "Alternatives" 메뉴(menuitemradio, 첫 항목에 Recommended 태그, 항목별 mini 막대), Accept 기본 버튼과 수락 후 "✓ X accepted" + Undo
  - 메뉴는 바깥 클릭과 Escape로 닫힌다. 좁은 컨테이너에서는 액션이 다음 줄로 내려간다.
- AxDB 적용
  - AI 도우미 응답의 제안 쿼리 카드. 응답에 ```코드 블록이 여러 개면 첫 번째가 추천이고 나머지는 대안이다.
  - Accept는 "새 쿼리 탭에 삽입"이다(**실행 안 함**). Undo는 삽입한 탭을 닫는다. 내용을 수정했다면 확인을 받는다.
  - 신호는 모델 자신감이 아니라 **실제 검사**로 정한다. 0은 미검사, 1은 구문 파싱 OK(CodeMirror 파서나 드라이버 split), 2는 참조한 테이블·컬럼·라벨이 get_schema_context 스키마에 존재, 3은 사용자가 '검증' 버튼을 눌러 EXPLAIN(PG는 ANALYZE 없이, READ ONLY 트랜잭션)이 통과한 경우다. 드라이버가 EXPLAIN을 지원하지 않으면 2가 최대다. 막대 툴팁에 근거를 쓴다.
  - 오류 설명(explain_error)의 수정 쿼리 제안에도 같은 카드를 쓴다.

## B. Button → 앱 전체 공통 Button
- 원본 구조
  - variant: primary(어두운 그라디언트, 상단 밝은 edge border-box 그라디언트, 1px ring, 다크에서는 밝게 반전), secondary(surface+line), ghost(muted, hover 채움), destructive(연한 빨강 채움+빨간 글자)
  - size: sm 32/md 40/lg 48. icon과 iconRight를 받고, 아이콘만 있으면 정사각형이다.
  - loading: 왼쪽 아이콘을 spinner로 바꾸고, 라벨 위로 shine 띠가 sweep한다(background-clip:text, ::after attr(data-text)). 클릭은 막고 aria-busy를 단다.
  - morph: 라벨이나 아이콘이 바뀌면 너비가 WAAPI로 480ms 보간되고, 새 라벨과 아이콘이 blur·translate로 1회 진입한다. 첫 렌더에는 애니메이션이 없다.
  - active scale .97, reduced-motion이면 전부 끈다.
- AxDB 적용
  - 모든 버튼을 Button으로 통일한다(기존 aria-label과 data-testid는 그대로 전달).
  - Run은 브랜드 그라디언트 전용 variant('brand')를 쓴다.
  - loading: Run 실행 중, Test 연결 중, AI 응답 대기, 저장 중
  - morph: Copy→Copied(아이콘 교체), Test→"연결됨 ✓"/"실패", Run→Running…
  - 너비 애니메이션은 DESIGN.md 5절의 "layout 속성 애니메이션 금지"에 대한 **문서화된 예외**다. 작은 버튼에서 480ms 1회이고, 그리드와 캔버스 밖에서만 쓴다.

## C. Tool Call Card / Group → 에이전트 활동과 AI 도구 사용 기록
- 원본 구조
  - 카드 헤더: 아이콘 타일 26px, mono 도구 이름(최대 55% 말줄임), 요약, 상태(running: spinner + 0.1s 단위 라이브 타이머 / success: 체크 draw + 소요 시간 / error: "Failed"), chevron
  - 본문은 0fr→1fr로 열린다. Arguments와 Result는 JSON 하이라이트(key, str, num, lit 색)이고, 복사 버튼은 아이콘→체크로 1.4초 전환된다. 긴 블록은 220px에서 잘리고 아래에 fade와 "Show all"이 붙는다. 오류면 메시지 박스와 Retry를 보여 주고, 오류가 나면 자동으로 열렸다가 다음 실행 때 닫힌다(사용자가 직접 토글했다면 유지).
  - Group: 최근 아이콘 3개를 겹쳐 쌓고 "Used N tools · 2.4s"(실행 중에는 "Using tools…"에 shine)를 보여 준다. 실패 수를 표시하고, 펼치면 카드 목록이 나온다.
  - 실행 중일 때만 spinner, 타이머 interval, shine이 돈다.
- AxDB 적용
  - 에이전트 활동 패널: MCP 호출 하나가 카드 하나다. agent activity 이벤트의 도구 이름, 연결, 결정(allowed/denied/approved), 소요 시간을 쓴다. denied면 오류 톤에 "권한 거부"를 표시한다.
  - 인자와 결과는 **기존 redaction을 거친 값만** 보여 준다(비밀번호, 토큰, 시크릿). 결과는 행 수 요약과 최대 20행만 보여 준다.
  - Retry는 읽기 도구(run_query read_only, explore, browse, get_*)에만 붙인다. 쓰기와 preview는 승인 절차를 우회할 수 있으므로 붙이지 않는다.
  - AI 도우미: 응답을 만들 때 쓴 맥락(스키마 맥락, 작업 맥락)을 ToolCallGroup으로 보여 준다(투명성).
  - 타이머는 실행 중일 때만 interval을 돌리고, 끝나면 해제한다(유휴 0).
