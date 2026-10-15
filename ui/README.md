# AxDB UI

Vite + Preact + strict TypeScript 프런트엔드입니다. 드라이버 선택 이후 모든 UI 동작은 manifest의 필드·언어·capabilities 및 반환된 결과 뷰로 결정합니다.

```sh
pnpm --dir ui install
pnpm --dir ui dev
pnpm --dir ui build
pnpm --dir ui exec tsc --noEmit
pnpm --dir ui test
```

브라우저에서는 mock을 사용하며 화면 상단에 MOCK 표시가 나옵니다. `window.__TAURI_INTERNALS__`가 있는 Tauri에서는 실제 invoke 명령을 사용합니다. mock 프로필·이력은 메모리에만 유지되고 새로고침하면 초기화됩니다.

- Analytics sandbox 연결 후 `SELECT * FROM people;`로 표를 확인합니다.
- `SELECT large;`와 row limit `100000`으로 10만 행을 확인합니다. 기본 한도는 1000입니다.
- `SELECT error;`, `SELECT slow;`, `SELECT disconnect;`로 오류·취소/타임아웃·재연결을 확인합니다.
- `SELECT documents;`는 큰 정수가 들어 있는 문서 원문을 반환합니다.
- Graph playground 연결 후 기본 쿼리를 실행하면 Graph/Table을 전환할 수 있습니다. 노드를 더블클릭하거나 속성 패널에서 이웃을 확장합니다.
- 탐색 트리는 펼칠 때 조회하고, 노드의 ⋯ 메뉴 또는 우클릭으로 Browse/Open query/Insert를 실행합니다. 더블클릭은 첫 번째 지원 액션을 실행합니다.
- Cmd/Ctrl+Enter는 선택 영역이 있으면 선택 영역만 실행합니다.
- 큰 숫자 파라미터는 문자열로 넣고 쿼리에서 캐스팅합니다. 결과 Int/Decimal/Json 원문은 JS 숫자로 변환하지 않습니다.
- 저장하지 않은 비밀번호는 연결 시 세션용 password dialog에서 입력합니다. 프로필 저장은 연결을 자동으로 바꾸지 않습니다.

구현·검증 증거와 번들 크기: [W1-report.md](W1-report.md).
