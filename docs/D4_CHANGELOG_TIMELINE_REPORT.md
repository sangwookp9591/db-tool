# D4 Changelog Timeline

2026-10-19 · `/Users/iron/Project/psw/db-tool` · 기존 변경이 있는 공유 main 작업 트리에서 D3 변경을 이어서 수정했다. 사용자 앱, GUI, DB, Cargo/Tauri 빌드, commit/push는 수행하지 않았다.

## 구현

사용자 제공 21st.dev Changelog Timeline(CCgather demo)을 `ui/src/ui/ChangelogTimeline.tsx`와 `.css`로 포팅했다. Preact hooks, lucide-preact, BEM CSS와 기존 midnight/daylight 토큰을 사용하며 새 의존성은 없다. 날짜 내림차순, 월 구분선, 레일·활성 강조, 호버 펼침, 클릭 고정, 방향키/Home/End/Esc, 접근성 속성 및 다음 줄 절반 노출을 유지했다. Esc는 고정을 해제하며 부모 대화상자를 닫지 않는다.

live 점과 첫 노드의 breathe는 각 3회(총 7.2초)로 제한했다. 첫 노드의 애니메이션을 호버 때마다 재시작하지 않으며 reduced-motion에서는 모든 타임라인 애니메이션을 끈다. 분 단위 interval은 없고 상대 시간 기준은 마운트 때 한 번 계산한다.

“새로운 기능”은 `docs/release-notes/v*.md`를 lazy glob으로 읽어 모든 버전의 bullet을 표시한다. 업데이트 대화상자는 서버가 제공한 새 버전 본문만 표시한다. 항목의 첫 문장은 제목, 나머지는 요약이며 new/improved/fixed와 기존 good/neutral/bad를 해석한다. HTML은 텍스트로 남고 링크는 기존 safeExternalUrl·openReleaseLink 경로를 사용한다. 사용처가 없어진 ChangeSummary 두 파일을 제거했다.

v0.2.0에 날짜 주석과 새 태그를 넣고 오류 수정 2개 항목은 fixed 섹션으로 분리했다. manifest는 새/이전 태그 및 날짜 주석을 제거하며 테스트에 해당 케이스를 추가했다. Vite 설정은 Node 타입 패키지를 추가하지 않기 위해 `vite.config.ts`에서 `vite.config.mjs`로 옮겼다. 빌드 시 짧은 Git HEAD와 릴리스 날짜를 주입하며, 날짜 주석이 없으면 해당 Git 태그의 생성 날짜를 사용한다. 날짜를 확인할 수 없으면 임의 날짜 대신 빌드 오류를 낸다. dev에서는 UI와 release-notes 디렉터리만 명시적으로 허용해 UI 루트 밖의 raw 동적 import도 작동하게 했다.

## 검증

- `pnpm --dir ui exec tsc --noEmit`: 최종 통과. 초기 검사에서 나온 공유 VirtualTable 타입 오류는 해당 소유 작업자가 해소했다.
- `pnpm --dir ui exec vitest run src/update/update.test.ts`: 6개 통과.
- `python3 scripts/test-release-manifest.py`: 통과; 기존 테스트 안에서 6개 태그와 날짜 주석 제거, 메타데이터만 있는 빈 노트 거부를 확인했다.
- `pnpm --dir ui exec vite build --outDir /tmp/db-tool-d4-final`: 최종 통과. `v0.2.0-*.js` 별도 청크 약 2.98 kB, gzip 1.51 kB. 브라우저에서도 첫 화면에 WhatsNew/ChangelogTimeline/릴리스 노트 요청이 없음을 확인했다.
- Playwright mock: 20개 실제 항목, 호버·고정·고정 해제, 키보드 이동, 정확한 다음 행 1/2 노출, 360/600/960px 수평 overflow 없음, 7.2초 이후 유휴 애니메이션 0, reduced-motion 0, 새 버전 0.3.0 항목만 표시를 확인했다. 별도 입력으로 날짜 내림차순과 2개 월 구분선, HTML 리터럴, 위험 링크 거부, 안전 링크 callback, 닫힌 본문의 inert도 확인했다.
- 전체 UI 검사 이력: dev 서버와 동시에 실행한 첫 검사는 137 통과/2 실패/1 skip(브라우저 timeout), 두 번째는 138 통과/1 실패/1 skip(onboarding의 Preact `__H` 인스턴스 충돌)이었다. timeout을 늘리거나 테스트를 수정하지 않았다. D4 dev 서버를 종료한 최종 실행도 138 통과/1 실패/1 skip이었다. 남은 실패는 `onboarding.test.ts:157`의 `installs a selected driver in place and locks navigation until it finishes` 5초 timeout이다. 따라서 전체 UI 게이트는 통과로 보고하지 않는다.

## 산출물과 범위

`docs/design-shots/changelog-timeline-{midnight,daylight}.png` 및 각 `-hover.png`, `-pinned.png` 총 6장. 추가 검증 결과는 `docs/design-shots/changelog-timeline-check.json`이다. 캡처는 headless Chromium의 Vite mock 증거이며 네이티브 앱·외부 GitHub 열기·실제 업데이트 설치 증거는 아니다.

D4 소유 변경: ChangelogTimeline 2개, ChangeSummary 2개 삭제, update의 WhatsNew/UpdateDialog/ReleaseNotes/notes/update.css/update.test, Vite 설정 이동, release notes, release-manifest와 그 테스트, THIRD_PARTY_NOTICES, 이 보고서와 위 캡처·JSON. App, Updater, host, useWhatsNew 및 L1/W4b의 기존 변경은 수정하지 않았다. 원본에는 별도 라이선스 본문이 없어 출처와 해당 사실을 THIRD_PARTY_NOTICES에 기록했다.
