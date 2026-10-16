# D2 · Preact micro interactions

2026-10-16. 이전 워커의 9종 구현, 제어 API, 접근성 별칭, gallery 진입과 핵심 테스트를 유지했다. 추가 6종과 후속 요청인 IntegrationBeam을 같은 디렉터리에 구현하고 실제 동작을 gallery에서 연결했다. D1 소유의 App·Connections·Results·style.css·package.json은 수정하지 않았다. 새 의존성, commit, push, 사용자 DB 접속은 없다.

## API

공통으로 `ariaLabel` / `aria-label`, `testId` / `data-testid`를 받으며 명시적인 HTML 속성 이름이 우선한다. 입력 컴포넌트는 전달받은 값이 상태의 기준이고, callback으로 변경을 요청한다. 컴포넌트는 같은 이름의 파일에서 named export한다. 스타일은 `micro.css` 하나이며 색은 DESIGN 토큰을 사용한다.

| 컴포넌트 | 주요 props | 접근성·수명 |
|---|---|---|
| JellyRadio | `items: (string \| {value,label,disabled?,ariaLabel?,testId?})[]`, `value`, `onChange`, `role?: radiogroup \| tablist`, `disabled?` | 항목에도 HTML 별칭 전달, roving tabIndex, 화살표·Home·End, 비활성 건너뛰기 |
| SwipeRow | `open`, `onOpenChange`, `actions: {id,label,icon?,disabled?,onSelect}[]`, `children`, `label`, `disabled?` | 드래그·deltaX, hover 버튼, 토글 ArrowRight/ArrowLeft/Escape. 닫힌 액션 rail은 inert |
| CometDial | `value`, `onChange`, `min=1`, `max=600`, `step=1`, `unit='s'`, `disabled?` | slider, aria-valuenow, 로그 눈금, 드래그·휠·화살표·Home/End·PageUp/Down |
| WakeSlider | 같은 slider props, 기본 `min=100,max=100000,step=100` | slider와 숫자 입력, clamp, 드래그 물결, 키보드 |
| CodeSlots | `value: string`, `onChange`, `length=6`, `mask?`, `disabled?`, `status?: idle \| error \| success` | 네이티브 입력·붙여넣기·OTP autofill, 숫자 정제, 오류 aria-invalid, success 읽기 전용 |
| NumberSlots | `value: number \| string`, `suffix?` | `CodeSlots.tsx`에서 export, 값이 달라진 자리만 spring, 전체 값 접근성 이름 |
| LatticeLoader | `running`, `label?` | running=false는 null, 타이머 해제. 실행 중에만 격자 무한 모션, 시간은 aria-hidden |
| BorderGlow | `children`, `active=true` | pointermove를 rAF 1회로 병합, leave·비활성·unmount 취소 |
| SpringCheck | `checked`, `onChange`, `label`, `disabled?` | 네이티브 checkbox, Space, label 연결, input에 testId 전달 |
| FuseButton | `pending`, `onPendingChange`, `onCommit`, `onUndo`, `duration=5000`, `unmountAction=commit`, `label?`, `doneLabel?`, `undoLabel?`, `disabled?` | deadline에서 한 번 commit, Undo/Escape는 취소. `unmountAction='cancel'`이면 언마운트 시 삭제하지 않음 |
| CommandPalette | `open`, `onOpenChange`, `items: {id,label,group?,keywords?,disabled?,onSelect}[]` | 기존 Dialog 재사용, ⌘K/Ctrl+K, combobox/listbox, NFC 검색, 화살표·Enter·Escape. 다른 모달 위에는 자동으로 열지 않음 |
| ToastStack | `items: {id,message,tone?: success \| error \| info}[]`, `onDismiss(id)` | status/alert, 명시적 닫기. 자동 소멸 타이머 없음 |
| Tooltip | `open`, `onOpenChange`, `content`, `children: 단일 VNode` | 기존 트리거 이름·handler 유지, aria-describedby 추가, hover/focus/Escape, native popover로 clipping 방지 |
| SlidingIndicator | `value: string`, `children` | 자손의 aria-selected/aria-checked 활성 항목을 따라 translate·scale. 기존 역할은 변경하지 않음 |
| SpotlightCard | `children`, `active=true` | 포인터 rAF 병합, 정적 focus 경계, 유휴 rAF 없음 |
| Skeleton | `running`, `rows=3` | status, 최대 10줄, 160ms 진입 후 정지, running=false는 null |
| IntegrationBeam | `nodes: {id,label,side: source \| target,state: connected \| disconnected \| pending \| denied,icon?}[]`, `controllerRef`, `onSelect(id)`, `hubId=axdb`, `hubLabel=AxDB`, `status?` | `ref.current.pulse(fromId,toId,tone?)`, tone은 primary/success/warning. hub↔node만 허용, invalid/missing geometry는 false, reduced motion은 이동 없이 true. 상태는 props로 제어하고 타일은 상세 열기 버튼 |

`FuseButton`은 부모가 pending을 true로 바꾸어 퓨즈를 시작한다. 도중 pending=false로 바꾸면 해당 작업을 조용히 취소한다. 재렌더링은 이미 시작한 deadline을 연장하지 않는다. 대화상자의 Cancel이 실제 삭제를 일으켜서는 안 되는 호출부는 `unmountAction='cancel'`을 써야 한다. 기본 commit 정책과 명시적 cancel 정책은 가짜 타이머로 검증했다.

## 추가 인터랙션 선택 근거

| 선택 | DB 작업 흐름에서의 이유 | 조사한 출처와 적용 범위 |
|---|---|---|
| 명령 팔레트 | 연결·테이블·최근 쿼리를 한 검색 흐름으로 이동 | [21st.dev Command](https://21st.dev/@shadcn/components/command), [affectUI Command Menu](https://www.affectui.com/components/command-menu). 독립 구현, DB 작업 실행은 caller의 onSelect |
| 슬라이딩 표시줄 | 탭·세그먼트 전환 후 현재 위치를 바로 확인 | [affectUI Segmented Control](https://www.affectui.com/components/segmented-control). 독립 구현, 선택만 따라가는 짧은 모션 |
| 스포트라이트 카드 | 조밀한 사이드바에서 현재 포인터 카드 구분 | [React Bits SpotlightCard](https://reactbits.dev/components/spotlight-card). ambient·grain·주변 카드 추적 없이 한 카드에만 적용 |
| 토스트 스택 | 실행 완료·복사·오류 피드백을 작업 영역 밖에 유지 | [affectUI Toaster](https://www.affectui.com/components/toaster). 공개 동작 설명 참고, 유료 소스 복사 없음 |
| 툴팁 | 아이콘 버튼의 의미와 단축키를 focus로도 확인 | [21st.dev Tooltip](https://21st.dev/@shadcn/components/tooltip). 독립 구현, 트리거 이름은 유지 |
| 로딩 스켈레톤 | 로딩 중 헤더·패널의 자리와 구조 안내 | [affectUI Shimmer Loader](https://www.affectui.com/motion/shimmer-loader). 반복 shimmer를 제외한 정적 skeleton |
| Integration Beam | 연결과 에이전트 사이에서 실제로 발생한 활동의 방향 표시 | [사용자 제공 명세](design-shots/ref-integration-beam.md)와 이미지. 이벤트에서만 한 번 재생하고 상태·권한은 caller가 제어 |

React Bits의 [micro/JellyRadio](https://reactbits.dev/micro/jelly-radio), [text-animations/CountUp](https://reactbits.dev/text-animations/count-up), [backgrounds/DotGrid](https://reactbits.dev/backgrounds/dot-grid)도 조사했다. 원본 9개 JSON의 TypeScript와 plain CSS를 읽고 입력·취소·수명 동작을 확인했다. 숫자 모션은 NumberSlots로 통합했으며 장식용 배경·캔버스 상호작용은 조용한 DB 작업대와 성능 예산에 맞지 않아 추가하지 않았다. affectUI는 공개 카탈로그를 Playwright로 확인했으며 원본 라이선스가 다른 소스를 가져온 것으로 표시하지 않았다. 고지는 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)에 있다.

## 사용자 추가 요청: Integration Beam

[제공된 명세](design-shots/ref-integration-beam.md)와 [이미지](design-shots/ref-integration-beam.png)를 읽고 이벤트 기반으로 독립 구현했다. 연결 프로필·에이전트 노드와 상태를 props로 받고, 호출부가 실제 이벤트에 맞춰 `controllerRef.current?.pulse(profileId, 'axdb', 'primary')` 또는 역방향 pulse를 요청한다. 노드 ID는 서로 및 hubId와 달라야 한다. 클릭은 `onSelect(id)`로 상세 화면을 열며 연결이나 권한을 몰래 토글하지 않는다.

ResizeObserver에서 타일 중심과 cubic 경로를 측정하고 좁은 폭에서는 세로로 배치한다. 반복 타이머와 rAF 루프는 없고, WAAPI transform/opacity로 650ms 빔과 도착 760ms ring만 재생한다. 완료·언마운트·reduced-motion 전환 시 임시 DOM과 애니메이션을 제거하며 동시 모션은 12개로 제한한다. Playwright에서 20개 이벤트를 연속 발생시켜 12개 제한과 실행 중 reduced-motion 전환에 따른 즉시 제거를 확인했다. 원본의 path morph·stroke-dashoffset·너비 전이는 DESIGN 5절에 맞춰 정적인 경로 갱신으로 대체했다. 갤러리의 버튼은 시뮬레이션 이벤트라고 명시했다. source code를 제공받거나 복사한 것으로 주장하지 않는다.

D1의 실제 cold trace에서 지적한 SlidingIndicator 최초 layout 강제 측정을 제거했다. 첫 위치는 ResizeObserver callback에서 읽고, 이후 선택 변경에만 spring을 적용한다.

## 검증

`pnpm --dir ui test`: 10개 파일, 53개 테스트 통과. `pnpm --dir ui exec tsc --noEmit`: 통과. spring 수렴과 유한 rAF, reduced motion, fuse deadline/undo/unmount, slider clamp/keyboard, radio keyboard, 명령 검색과 빔 곡선의 양방향 좌표를 검증한다. `pnpm --dir ui exec vite build --outDir /tmp/axdb-d2-dist`도 통과했다.

브라우저 mock dev 서버의 `?gallery`에서 `node ui/src/ui/gallery-check.mjs`를 실행했다. 모든 16종을 실제 포인터·wheel·키보드로 조작했고 **3초 유휴의 document.getAnimations()=0, rAF callback=0**, reduced motion에서 animation=0, 페이지 오류=0을 확인했다. 320px·768px의 문서 가로 overflow도 없다. 공유 트리 편집 중 HMR로 화면 상태가 리셋되는 간섭을 피하기 위해 최종 검증은 별도 Vite dev 서버에서 HMR을 끄고 `GALLERY_URL`로 지정했다. 자세한 assertion은 스크립트에 있고 결과는 [d2-micro-check.json](design-shots/d2-micro-check.json), 화면은 [Midnight](design-shots/d2-micro-dark.png) / [Daylight](design-shots/d2-micro-light.png)에 있다. 두 스크린샷은 직접 열어 확인했다.

기존 코드를 유지하면서 BorderGlow 비활성 전환 시 예약 프레임 취소, FuseButton 실행 중 reduced-motion 전환 시 WAAPI 취소, reduced-motion CSS의 컴포넌트 root 누락을 보완했다. 팔레트의 Enter 선택 직후 Ctrl+K 재열기가 실패하는 것을 재현했고, 이전 open 상태를 캡처한 지연 effect를 layout effect로 바꾸어 재열기를 확인했다. 카드의 메뉴는 강조 레이어 밖으로 펼칠 수 있고, 표시줄은 스크롤된 탭의 위치와 보이는 폭을 반영한다. 새 스타일은 transform/opacity만 애니메이션하며 반복 애니메이션은 실행 중 LatticeLoader 하나다. 새 컴포넌트에 driver-id 분기와 motion/gsap/hugeicons import는 없다.

## 검증 범위와 통합

공유 작업 트리에는 시작 시 이미 Rust·UI·테마·폰트·D1 통합 변경이 있었다. 이 보고서의 D2 범위는 `ui/src/ui/**`, 기존 main.tsx gallery 진입 한 줄, DESIGN 5절, notices의 추가 고지, 이 보고서와 `docs/design-shots/d2-*`이다. 타 작업자의 변경을 D2 성과로 계산하지 않는다.

프로덕션 JS gzip은 baseline 172,157 bytes에서 188,876 bytes로 **16,719 bytes 증가**해 40KB 예산 안이다. [d2-bundle-check.json](design-shots/d2-bundle-check.json)은 `--emptyOutDir`로 이전 빌드 파일을 제외한 결과다. D1의 baseline 산출물과 당시 공유 작업 트리의 빌드 비교이므로 D2만의 증가량이 아니다. 이 수치는 후속 IntegrationBeam의 App 호출부 연결 전 스냅샷이며 최종 통합 번들 측정은 D1에서 갱신해야 한다. 개발 전용 gallery는 프로덕션 빌드에서 제외된다. D1이 추가 6종을 App·Connections·Results·QueryTab에 연결한 호출부를 확인했고, [통합 mock 팔레트](design-shots/d2-workspace-palette.png)의 실제 버튼 클릭·검색도 확인했다.

IntegrationBeam 추가 전 15종 갤러리의 [60초 브라우저 유휴 측정](design-shots/d2-browser-idle-60s.json)은 Chromium renderer의 CDP TaskDuration 차이 0.021854초 / 60.004289초, **0.0364%**, animation=0이었다. 이는 네이티브 프로세스 CPU가 아니며 W4 그리드/그래프, 네이티브 첫 페인트, 최종 .app 크기 검증을 대신하지 않는다. 그 통합 측정은 D1 범위로 남긴다.

네이티브는 D1이 `CARGO_TARGET_DIR=/tmp/axdb-d1-target`으로 빌드하고 지정된 테스트 설정·secret store·bridge 환경으로 실행한 embedded debug 앱(pid 36270)을 사용했다. 팔레트의 실제 열기·검색·Escape 닫기, 슬라이더 표시와 숫자 입력값 동기화를 확인했다. [네이티브 팔레트](design-shots/d2-native-palette.png), [네이티브 슬라이더](design-shots/d2-native-slider.png), [실행 환경과 검증 범위](design-shots/d2-native-check.json)를 남겼다. 이후 D2가 하지 않은 연결·탐색 변화가 나타나 GUI 조작을 중단하고 D1에 반환했다. 따라서 네이티브의 정확한 한 단계 키보드 증가와 전체 16종 E2E를 통과했다고 주장하지 않는다. 브라우저에서는 해당 키보드 로직이 검증됐다. IntegrationBeam은 이 네이티브 캡처 이후 추가되었으므로 그 캡처가 빔의 실제 이벤트 연결을 증명하지는 않는다. Graph node limit은 원래 300에서 포인터로 2501로 바뀌었고 후속 화면은 2224였다. GUI 반환 메시지에 이를 알리고 D1에 원복을 요청했다. 이 한도는 App의 메모리 상태여서 앱을 재시작하면 기본 300으로 돌아온다. D2는 DB 연결·쿼리·삭제를 실행하지 않았다.
