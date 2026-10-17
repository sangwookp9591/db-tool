# U1-ui-fixes: 전체 UI 겹침·가려짐·깨짐 수정

근거: [UI_AUDIT_NATIVE.md](../UI_AUDIT_NATIVE.md)(아래 `#번호`), 자동 측정 보고(`F번호`, 코디네이터가 경로 전달), 사용자 제보(탭 바 스크린샷). 기준: [DESIGN.md](../DESIGN.md).

## 공통 원인: `:root .ax-button` 우선순위 회귀 (사용자 제보)
디자인 v3의 `:root .ax-button`(테두리·radius·배경, 명시도 0,2,0)이 그 전의 `.workspace-tab>button{border:0;background:transparent;border-radius:0}`(0,1,1) 같은 보정 규칙을 이긴다. 그래서 활성 탭의 제목과 닫기 X가 테두리 있는 상자 두 개로 떨어지고, 위쪽 그라데이션 선·아래 슬라이딩 선과 어긋난다. `ui/src/style.css`에서 같은 위험이 있는 선택자:
`.rail>button`, `.section-title>button`, `.connection-actions>button`, `.connection-menu button`, `.explorer .toolbar button`, `.tree-node>button:last-child`, `.context-menu button`, `.notice button`, `.workspace-tab>button`, `.view-switch button`, `.result-tabs button`, `.column-header button`, `.workspace-error>button`, `.history-list>button`, `.agent-controls>button`, `.agent-rail-button`, `.document-row button`, `.row-inspector>button`, `.history .section-title button`, `.rail>.micro-tooltip>button`, `.dialog-header button`.
각 위치에서 실제로 `.ax-button`이 붙는지(Button이 bare가 아닌지) 확인하고, 깨진 곳은 고친다. 방법은 둘 중 위치에 맞는 것: (a) 납작해야 하는 컨트롤은 `<Button bare>` 또는 전용 클래스, (b) 보정 규칙의 명시도를 `:root …` 형태로 올림(파일에 이미 쓰는 관례). 한 가지 방식으로 일관되게 하고, 보고서에 위치별 판정표를 남긴다.

## 공통 제약
- **aria-label, role, 키보드 단축키를 바꾸지 않는다**(DESIGN §152, E2E와 에이전트 레지스트리가 의존). 새 컨트롤에는 새 aria-label.
- 드라이버 id 분기 금지(grep 0 유지). 디자인 토큰만 사용. 편집 모드가 꺼졌을 때 VirtualTable 렌더 경로 유지(W4 성능).
- 같은 작업 트리에서 세 워커가 병렬로 일한다. 공유 파일(특히 `ui/src/style.css`)은 **수정 직전에 다시 읽고, 작은 국소 수정만** 한다. 파일 전체 재작성·서식 일괄 변경 금지.
- `pnpm --dir ui build`로 `ui/dist`를 쓰지 않는다. 빌드 확인은 `pnpm --dir ui exec vite build --outDir <scratch>`. 커밋하지 않는다(코디네이터가 워커별로 커밋).
- 문구 언어(#11)는 사용자 결정 대기다. 이번 작업에서는 새로 추가하는 문구만 주변과 같은 언어로 쓰고 기존 라벨의 언어는 바꾸지 않는다.

## 검증(각 워커 공통)
`pnpm --dir ui exec tsc --noEmit`, `pnpm --dir ui test`, scratch outDir 빌드 통과. 브라우저 mock(ego-browser)에서 담당 화면을 1280×800·1024×700·800×600, Midnight·Daylight로 전후 스크린샷. 자동 측정 스크립트(`overlap-probe.js`, 코디네이터가 경로 전달)를 담당 화면에 다시 돌려 해당 F/# 항목이 사라졌는지 확인. 보고: `docs/U1_<영역>_REPORT.md`.
