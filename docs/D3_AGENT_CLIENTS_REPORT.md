# MCP 클라이언트 · 새로운 기능 화면

2026-10-19 · main `24dddf5` 기반 공유 작업 트리. 사용자 앱·GUI·실제 DB에 접근하지 않았고 commit/push 또는 네이티브 빌드는 하지 않았다.

MCP 대화상자의 상단을 브리지 토글·주소·클라이언트 수·주 행동으로 정렬했다. 클라이언트가 없으면 3단계 안내와 생성 버튼을, 있으면 권한 배지·최근 감사 활동·토큰 회전·연결 해제가 있는 카드를 보여 준다. 카드를 선택하면 연결별 권한 편집으로 전환한다. 노출 도구와 감사 로그는 접힘 섹션이며, 감사 기록이 없으면 빈 상태를 표시한다. 호출 이력은 기존 CallChip/ToolCallGroup을 재사용하고 감사 결정에는 색 칩을 붙였다. 푸터는 오른쪽 보조 닫기 버튼 하나다.

브리지 전체 끄기와 개별 토큰 폐기를 구분했다. 기존 FuseButton의 취소 시간·확인 창과 토큰 회전으로 폐기된 클라이언트를 다시 활성화하는 동작을 보존했다. 수치는 소켓 연결 수로 오인하지 않도록 등록된 클라이언트 수와 유효 토큰 수로 표시한다. 설정 복사는 실제 브리지 주소와 방금 생성/회전한 해당 클라이언트 토큰만 사용한다. 기본 로컬 토큰이 폐기되면 기존 `onboarding_mcp_config`가 주소 조회도 거부하므로 오류를 표시하고 설정 복사를 비활성화한다. 기본 로컬 토큰 회전으로 복구할 수 있으며, 다른 클라이언트의 토큰을 대신 복사하지 않는다.

사용자가 추가 제공한 TeamInvite는 빈 상태·새 연결·생성 완료 장면에, AgentTeam은 권한 편집 상단에 넣었다. 두 모듈은 lazy import이며 로컬 AxDB/Lucide 아이콘과 테마 토큰을 사용한다. IntersectionObserver로 보일 때 한 번만 재생하며 offscreen·문서 숨김·닫힘·unmount 시 정리하고 reduced-motion은 완료 정지 프레임이다. Preact 11의 passive effect cleanup이 지연되는 것을 확인해 layout effect로 즉시 정리한다. 예시 그림이 실제 연결이나 권한 부여를 의미하지 않도록 설명과 접근성 이름을 붙였다. 자연어 `plan_grants` 입력은 연결하지 않았다. 실제 MCP 권한은 기존 연결별 편집으로 지정하며 DB 역할 계획 기능은 변경하지 않았다.

ChangeSummary는 제공된 행 디자인을 Preact와 별도 CSS로 옮겼다. 릴리스 노트의 `##`마다 제목·첫 항목 요약·톤을 표시하고, 행을 펼치면 기존 안전한 Markdown 렌더가 전체 본문을 보여 준다. `{good|bad|neutral}` 선택 태그는 화면에서 제거하며, 태그가 없으면 추가/개선·제거/변경 주의·수정 문구로 톤을 정한다. 업데이트 대화상자와 설정 메뉴/명령 팔레트의 새로운 기능에 적용했다. 현재 릴리스는 `v0.2.0 · 2026.10.19`이며 버전별 최초 표시를 기기에 기록한다. 온보딩/다른 모달이 열려 있으면 자동 표시를 기다린다. 모의 모드는 `?changelog`에서만 자동 표시하며 일반 자동화 화면을 방해하지 않는다.

## 검증

- `pnpm --dir ui exec tsc --noEmit`: 통과.
- 첫 `pnpm --dir ui test`: 33개 파일 중 31개 통과, 133개 테스트 중 130개 통과. 기존 onboarding 2개와 BranchedMenu 1개가 브라우저 hook/test timeout으로 실패했다.
- 다른 캡처 작업을 종료한 뒤 `pnpm --dir ui test --maxWorkers=1`: 33개 파일 / 134개 테스트 통과, 52.64초. timeout 증가·강제 종료·retry 설정은 추가하지 않았다. 새 비단순 파서의 섹션/톤/본문 보존 경계만 기존 update.test.ts에 한 건 추가했다.
- 별도 Vite mock(`127.0.0.1:5197`) + headless Playwright: MCP 빈 상태·생성·카드 편집/저장·브리지 실패 후 꺼짐 유지·감사 빈 상태, 960×600 가로 넘침 없음과 푸터 접근, scroll snap 없음 확인.
- 변경 로그: 최초 1회 표시와 재실행 미표시, 메뉴로 다시 열기, 키보드 펼침, 전체 본문, HTML literal 처리, javascript 링크 차단, 안전한 링크 콜백 확인.
- 일러스트: 초기 앱 로드에 두 모듈 미요청, offscreen 미재생, visibilitychange 취소, 재등장 시 반복 없음, unmount 즉시 scene timer 0, reduced-motion 완료 프레임·애니메이션 0 확인. Preact 자체 스케줄러 타이머와 장면의 2800/3400ms 타이머를 구별했다.
- 상세 브라우저 결과: [검증 JSON](design-shots/d3-agent-clients-check.json). idle 애니메이션 0이며 새 장면은 요청된 유한 진입 연출 이후 정지한다.

## 캡처

| 상태 | Midnight | Daylight |
|---|---|---|
| 빈 상태 | [보기](design-shots/agent-clients-empty-midnight.png) | [보기](design-shots/agent-clients-empty-daylight.png) |
| 클라이언트 목록 | [보기](design-shots/agent-clients-list-midnight.png) | [보기](design-shots/agent-clients-list-daylight.png) |
| 권한 편집 | [보기](design-shots/agent-clients-edit-midnight.png) | [보기](design-shots/agent-clients-edit-daylight.png) |
| TeamInvite | [보기](design-shots/agent-clients-invite-midnight.png) | [보기](design-shots/agent-clients-invite-daylight.png) |
| AgentTeam | [보기](design-shots/agent-permissions-team-midnight.png) | [보기](design-shots/agent-permissions-team-daylight.png) |
| ChangeSummary | [보기](design-shots/changelog-midnight.png) | [보기](design-shots/changelog-daylight.png) |

캡처는 mock fixture 증거다. 실제 브리지 켜기/토큰 발급·폐기/외부 에이전트 접속, 네이티브 WebKit, 앱 업데이트 설치·재시작 및 실제 DB 권한은 실행하지 않았다. localStorage를 사용할 수 없으면 새로운 기능의 자동 표시는 생략되고 메뉴 접근은 유지된다.

## 변경 범위

초기 소유 범위는 `ui/src/agent/ClientsPanel.tsx`와 새 `clients.css`다. 추가 지시에 따라 `ui/src/ui/{ChangeSummary,TeamInvite,AgentTeam}.{tsx,css}`, `useIllustration.ts`, `ui/src/update/{ReleaseNotes,Updater,WhatsNew}.tsx`, `notes.ts`, `update.css`, `update.test.ts`, 릴리스 노트와 출처 문서를 수정/추가했다. `App.tsx`에는 새로운 기능 hook·대화상자·설정 메뉴·명령 팔레트 진입과 기존 StarPrompt 차단 조건만 연결했다.

시작부터 Cargo/Rust/백업/측정/문서 및 UI BackupRoles·administrationContract·mock·branched-menu.test 변경이 존재했다. 이 워커는 해당 변경을 수정하거나 되돌리지 않았다. 자체 Vite 서버와 headless 브라우저만 종료한다.
