# D3 · 공통 Button / Smart Recommendation / Tool Call

2026-10-16. [사용자 제공 명세](design-shots/ref-components-batch2.md)를 Preact와 기존 AxDB 토큰으로 재구현했다. 각 새 소스에 `사용자 제공 레퍼런스 재구현`을 표기했고, 기존 `MicroAccessibility`와 `reducedMotion` 헬퍼 및 설치된 lucide 아이콘을 사용했다. 새 라이브러리는 추가하지 않았다.

## API와 통합

모두 파일별 named export이며 스타일은 각 컴포넌트가 `micro-extra.css`를 가져온다. `ariaLabel`/`testId` 별칭보다 명시적인 `aria-label`/`data-testid`가 우선한다.

| Export | API | 호출부 책임과 동작 |
|---|---|---|
| `Button`, `ButtonProps` | native button props와 DOM `ref`; `variant='secondary'`: primary/secondary/ghost/destructive/brand; `size='sm'`: sm/md/lg; `icon`, `iconRight`, `loading=false`, `morph=true`, `bare=false` | type 기본 button. 명시적인 type, disabled, onClick, aria-label, data-testid, name/value, class/style와 이벤트를 전달한다. loading은 disabled와 aria-busy를 설정해 클릭을 막는다. |
| `SuggestionCard`, `Suggestion`, `SuggestionCardProps` | `suggestions: {id,title,query,reason,signal:0\|1\|2\|3,evidence}[]`; `acceptedId?`, `onAccept(suggestion)`, `onUndo(suggestion)`, `disabled?`, `onValidate?(suggestion)`, `validating?` | 대안 선택은 내부 상태, 수락 여부는 acceptedId로 제어한다. 비동기 삽입/취소 성공 후에만 caller가 acceptedId를 갱신한다. 빈 배열이면 아무것도 표시하지 않는다. |
| `ToolCall`, `ToolCallProps` | `name`, `summary?`, `status: running\|success\|error`, `icon?`, `arguments?`, `result?`, `error?`, `durationMs?`, `startedAt?`, `decision?: allowed\|denied\|approved`, `retrySafe=false`, `onRetry?` | redaction을 마친 값만 전달한다. startedAt은 epoch ms이며 재실행마다 갱신한다. durationMs 미제공 시 완료 시간을 생략한다. denied는 권한 거부로 표시하고 Retry를 숨긴다. |
| `ToolCallGroup`, `ToolCallGroupProps` | `calls: (ToolCallProps & {id:string})[]`, `defaultOpen=false` | ID는 호출마다 고유해야 한다. 최근 아이콘 3개, 호출 수, 실패 수를 표시한다. 합계 시간은 모든 호출의 durationMs가 있는 경우에만 표시하며 병렬 호출의 wall time이 아닌 기록된 소요 시간의 합계다. |
| `toolPreview` | `(redactedValue: unknown) => string` | 화면과 복사가 같은 제한된 미리보기를 사용한다. 원본 전체 JSON export 용도로 쓰지 않는다. |

D1의 요청을 반영해 `onValidate`/`validating`, 실제 HTMLButtonElement ref 전달, `bare` 경로를 추가했다. `bare`는 내부 span·아이콘·스타일 클래스·observer를 만들지 않고 caller의 children을 native button에 그대로 넣는다. 그리드/트리 셀의 기존 DOM 수와 class/style을 보존할 때 사용한다. loading 클릭 방지와 aria-busy는 bare에서도 적용되지만 spinner/shine은 표시하지 않는다. `morph={false}`는 일반 버튼의 모양을 유지하면서 observer와 WAAPI를 제거한다. Run/Copy/Test 등 작은 버튼에만 morph를 켜고 Run에만 brand를 적용한다.

```tsx
<Button variant="brand" loading={running} morph aria-label="Run query"
  data-testid="run-query" onClick={run}>실행</Button>
<Button bare ref={cellButtonRef} className="existing-cell" onClick={openCell}>
  {existingCellContent}
</Button>
<SuggestionCard suggestions={checkedSuggestions} acceptedId={insertedSuggestionId}
  onAccept={insertIntoNewTab} onUndo={closeInsertedTab}
  onValidate={validateReadOnlyExplain} validating={validating} />
<ToolCall name="get_schema_context" status="success" result={redactedContext}
  durationMs={elapsedMs} />
```

Accept는 새 탭 삽입만 요청한다. SQL 실행·탭 생성·수정 여부 확인·취소 확인은 컴포넌트에 들어 있지 않으며 D1 호출부가 연결한다. signal은 컴포넌트가 추론하지 않는다. caller가 실제 검사 결과와 근거 문자열을 전달하고, `onValidate`는 선택된 제안을 넘긴다. 검사 완료 시 원래 검사한 suggestion ID에 결과를 연결해야 한다. 모델 자신감과 EXPLAIN 미실행을 검증 성공으로 표시해서는 안 된다.

ToolCall의 Retry는 caller가 안전한 읽기 작업임을 확인해 `retrySafe`와 `onRetry`를 모두 전달한 경우에만 나온다. 쓰기·preview·권한 거부에는 재시도를 연결하지 않는다. 자동 열림은 초기/전환 error 또는 denied에 적용되고 다음 running에 닫힌다. 사용자가 직접 열거나 닫은 경우에는 그 선택을 이후 상태 변화에도 유지한다. 그룹이 닫히면 자식 카드를 unmount하므로 그 안의 실행 타이머도 해제된다. 그룹 재개방에서도 정확한 실행 시간을 유지하려면 startedAt을 제공한다.

## 표시·모션과 보안 경계

색·폰트·반경은 AxDB 토큰을 사용한다. 버튼 primary는 테마에 따라 대비가 반전되고 brand는 기존 브랜드 그라디언트를 사용한다. 기존 micro 갤러리의 범용 button/section 규칙이 새 variant와 크기를 덮는 현상을 실제 스크린샷에서 발견해 새 CSS의 범위와 우선순위를 보완했다. 기존 CSS 파일은 변경하지 않았다.

Button 최초 렌더에는 morph를 하지 않는다. 라벨/아이콘 변화에는 유한 WAAPI를 적용하고, 작은 버튼 너비만 명세의 480ms 예외를 쓴다. grid/canvas 내부에서는 bare 또는 morph=false가 필수다. blur, grid-template-rows, check stroke-dashoffset 모션은 DESIGN의 transform/opacity 규칙에 맞춰 생략하고 180–220ms 진입 모션과 정적 레이아웃 변경을 사용했다. spinner/shine은 loading 또는 running일 때만 있고 reduced-motion에서는 끈다. WAAPI는 언마운트·설정 전환 시 취소한다.

JSON은 최대 8,000자/20줄, 배열 최대 20행, 객체 최대 20키, 깊이 6, 탐색 노드 200개로 제한한다. 긴 문자열은 1,000자, 키는 200자로 제한하며 순환 참조와 직렬화 실패도 처리한다. 미리보기는 잘린 JSON일 수 있으므로 파싱 가능한 원본이라고 주장하지 않는다. '미리보기 전체 표시'는 220px 접힘을 해제할 뿐 원본 한도를 해제하지 않는다. 오류 메시지는 최대 2,000자로 표시한다. 값은 Preact 텍스트 노드로 렌더하고 HTML 삽입을 하지 않는다. redaction 자체는 caller 책임이다. 복사 성공 표시의 1.4초 timeout은 동작 후 한 번만 사용하고 unmount 시 정리한다.

## 검증 증거

`pnpm --dir ui exec tsc --noEmit` 통과. `pnpm --dir ui test` 최종 실행에서 **13개 파일 / 64개 테스트 통과**했다. 이 수에는 공유 트리에서 병행 추가된 다른 lane의 테스트도 포함된다. D3의 `ui/src/ui/extra.test.ts` 5개 테스트는 기존 Playwright Chromium을 Vitest에서 실행하여 실제 DOM을 검증한다. 별도 DOM 라이브러리나 mock hook을 추가하지 않았다.

Button의 속성 전달·loading 클릭 차단·disabled·bare의 단일 DOM·객체 ref, SuggestionCard의 키보드 대안 선택·검증·accept/undo 실제 콜백 결과·Escape/바깥 클릭, ToolCall의 오류 자동 열림·재시도 자동 닫힘·수동 선택 보존·완료/언마운트 타이머 해제·denied Retry 차단·미기록 시간 생략을 확인했다. JSON 크기/행/순환 제한, 브랜드 그라디언트 및 40px 크기, 3초 유휴와 reduced-motion 로딩/실행도 assertion으로 검증했다. 개발 서버 자체 interval은 baseline으로 구분하고 도구 interval이 실행 중에만 하나 증가했다가 원복되는 것을 확인한다.

독립 Vite mock 갤러리를 Ego browser에서 조작하고 [브라우저 측정](design-shots/d3-browser-check.json)을 남겼다. 3초 유휴 `document.getAnimations()=0`, 추가 rAF callback=0, ToolCall의 100ms interval=0이었다. reduced-motion에서 loading/running 중 animation=0, 320px/768px 문서 가로 overflow=false다. [다크](design-shots/d3-dark.png)와 [라이트](design-shots/d3-light.png)를 저장하고 직접 열어 브랜드 색·카드·JSON·실패 상태를 확인했다. 갤러리에는 fixture임을 표시했고 실제 DB·EXPLAIN·탭 작업을 하지 않는다. 갤러리 브라우저와 D3가 시작한 서버는 종료했다.

## 변경 범위와 남은 검증

새 파일은 `ui/src/ui/Button.tsx`, `SuggestionCard.tsx`, `ToolCall.tsx`, `micro-extra.css`, `ExtraGallery.tsx`, `extra.test.ts`다. 기존 `gallery.tsx`에는 import와 `<ExtraGallery />` 두 줄만 추가해 기존 16종을 보존했다. 이 보고서와 `docs/design-shots/d3-*`는 요청된 산출물이다. 시작부터 Rust/UI/테마/D2 파일에 변경이 존재했고, 종료 시 assistant/types/perf 등 다른 lane 변경이 더 있었으나 D3는 해당 파일을 수정하지 않았다. commit/push는 하지 않았다.

실제 화면 통합과 신호 검사·탭 삽입/수정 확인·읽기 전용 EXPLAIN·권한 정책은 D1 책임이다. 앱 GUI, 실제 DB, 네이티브 성능/W4/60초 CPU/최종 번들 예산은 이번 D3 검증에서 실행하지 않았다. 테스트와 갤러리 결과는 이 경계를 넘는 증거가 아니다.
