# 온보딩 구현·검증 기록

## 구현 범위

환영 → DB 드라이버 선택 설치 → 연결·탐색 → ERD·뷰어 → 권한·안전 → MCP → 내보내기·AI의 7장면을 제공합니다. 2장면은 ADR-0003 결정에 따라 P3의 실제 `DriverPicker`를 사용하며 설치 중 이동과 닫기를 잠급니다. 설치된 드라이버가 0개여도 온보딩을 표시합니다.

`Dialog`(내부 BorderGlow 포함), `Button`, `JellyRadio`, `SpringCheck`, `IntegrationBeam`, 공식 로고와 테마 토큰을 재사용합니다. CSS 모션은 유한 transform/opacity 위주이며 장면이 바뀌면 이전 장면을 언마운트합니다. MCP 예시 펄스 타이머와 컴포넌트 애니메이션도 해제하며 축소 모션에서는 정적 화면을 제공합니다.

호스트의 `settings.json.onboarding_version`에 완료/건너뜀을 저장합니다. 기존 브리지 설정과 같은 저장 잠금·원자적 0600 파일 쓰기를 사용하고 다른 키와 더 높은 버전을 보존합니다. `AXDB_CONFIG_DIR`, `AXDB_BENCH=1`, `AXDB_AGENT_BRIDGE=1` 실행은 자동 표시를 생략합니다. 모의 브라우저는 기본 자동 표시를 생략하며 `?onboarding`에서 첫 실행을 재현합니다. 모의 완료는 해당 페이지 메모리만 사용하고 실제 앱 저장은 호스트 명령을 사용합니다.

MCP 토글은 실제 `agent_bridge_set`에 연결합니다. JSON 복사는 현재 실행 중인 브리지에서 URL과 유효한 현재 로컬 토큰을 얻으며, 토큰을 DOM·로그·설정 파일에 추가 저장하지 않습니다. 클라이언트 권한 범위 토큰은 기존 클라이언트 패널에서 발급합니다. 모의 화면은 실제 설정 복사 성공을 흉내 내지 않습니다.

## 검증과 재현

- `pnpm --dir ui exec tsc --noEmit`
- `pnpm --dir ui test --maxWorkers=1`
- `cargo test -p axdb onboarding --lib`
- 장면 캡처: `VITE_ONBOARDING_SHOTS=1 pnpm --dir ui exec vitest run src/onboarding/onboarding.test.ts -t 'fits both themes'`
- 캡처 위치: `docs/design-shots/onboarding-{1..7}-{midnight,daylight}.png`, 진입 모션 프레임 `onboarding-entry-{1..3}.png`

호스트 테스트 2개는 설정 보존·손상 파일 거부·꺼진 브리지 설정 거부를 통과했습니다. 브라우저 증거는 모의 백엔드의 Chromium 960×600 화면이며 실제 DB, 설치 패키지 검증, 네이티브 클립보드 또는 외부 MCP 클라이언트 연결 증거가 아닙니다. 네이티브 앱 GUI는 요청대로 실행하지 않았습니다.

## 최종 결과

- TypeScript: `pnpm --dir ui exec tsc --noEmit` 통과.
- 온보딩 Chromium: 최종 **10/10 통과** (81.37초, 캡처 모드 포함). 키보드 이동·건너뜀·설정 재표시·⌘K 재표시·모의 MCP 복사 보호·다크/라이트 7장면 최소 크기·축소 모션·종료 후 3초 애니메이션 0·설치 중 이동 잠금·빈 결과 안내·실제 결과/이미지 버튼 강조를 확인했습니다.
- 실제 모의 Neo4j 설치 → 연결 → 쿼리 결과의 E1 내보내기 두 버튼으로 이동해 포커스/강조를 확인했고, 자동 내보내기가 발생하지 않는 것도 확인했습니다. 결과가 없으면 샘플 쿼리와 연결 추가를 안내합니다.
- Rust 온보딩 범위: **2/2 통과**. 전체 Rust 작업공간이나 설치형 플러그인 통합 검사를 뜻하지 않습니다.
- 스크린샷 **14장 + 모션 3프레임** 생성. 일반 검사에서는 캡처를 저장하지 않으며 명시 환경변수가 있을 때만 생성합니다.
- 초기 전체 UI 기본 실행: 99/103 통과, 온보딩/기존 BranchedMenu timeout. 이후 전체 순차 실행: 102/105 통과, 온보딩 긴 사례 2개·기존 BranchedMenu 1개 timeout 및 두 Vite 종료 훅 timeout. 이 실패는 최종 단독 통과로 지우지 않습니다. 짧은 사례 분리와 레일 겹침 수정 이후의 최종 전체 1회 검증은 P3에 인계했습니다.

## 초기 검사에서 발견한 사항

브라우저 `evaluate` 안의 동적 import가 Vitest SSR helper로 변환되던 테스트 결함을 수정했습니다. `prefers-reduced-motion`을 닫은 직후 되돌리면 기존 빈 작업공간 로고의 `brand-arrival` 900ms 모션이 다시 시작되어 getAnimations가 1이 되는 현상은, 설정 복귀를 관측 뒤로 옮겨 온보딩 해제 검사와 분리했습니다. 라이트 테마의 공식 로고는 토큰 색 마스크로 표시하고, 공통 `.micro svg` 크기에 축소되던 방패는 장면 크기로 재지정했습니다.

초기 전체 병렬 실행에서는 온보딩 단계/재진입과 기존 BranchedMenu 검사에 5초 timeout이 있었고, 별도 실행에서는 Vite 종료 훅 timeout도 관측했습니다. 준비·실행·해제를 분리하고 페이지를 테스트마다 닫으며, 기본 테스트에서 14장면 캡처 대기를 제외했습니다. 테스트 제한을 늘리거나 재시도 옵션을 추가하지 않았습니다. 팔레트 포인터 경로의 반복 timeout은 P3의 드라이버 레일 버튼 추가로 검색 아이콘이 기존 고정 에이전트 버튼(top:192px)에 덮인 실제 UI 문제였습니다. P3가 드라이버 버튼을 rail-agent-slot 뒤로 옮겨 기존 위치를 보존하도록 수정했으며, 온보딩의 요청된 ⌘K 진입은 별도로 통과했습니다.

## 작업 경계

이 작업은 `ui/src/onboarding/`, `src-tauri/src/onboarding.rs`, App의 마운트·재표시 항목, lib의 명령 등록, agent_bridge의 현재 설정 접근/설정 읽기 공유, AgentPanel의 토글 상태 동기화에 한정됩니다. P3의 드라이버 UI/모의 설치, E1의 결과·이미지 내보내기, S1의 사이드바 변경은 각 담당 작업자의 범위입니다. commit/push는 수행하지 않았습니다.

시작 시 HEAD는 a3309a7이었고 기존 미추적 docs/RELEASE_SIGNING.md가 있었습니다. 병렬 작업 중 공유 HEAD/다른 파일이 바뀌었으며, 해당 변경을 덮어쓰거나 되돌리지 않았습니다.
