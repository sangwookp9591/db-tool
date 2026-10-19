# M4 안전 시점 WebView 재활용 평가

**평가 완료·제품 미채택.** 같은 창에서 WebView를 교체하면 앱 footprint가 시작 +30.8MiB, +22.0MiB로 내려가 두 준비 완료 지점에서 메모리 목표를 충족했다. 같은 호스트/드라이버 세션과 저장한 UI 상태도 복원됐다. 그러나 요청이 없는 숨김 상태의 후속 대기가 103.8초, 177.3초로 지연돼 비활성 상태에서 안정적으로 끝나는 조건을 충족하지 못했다. reload는 시작 +136.0MiB로 목표에 미달했다.

MCP 부하를 별도로 주었을 때는 reload/교체 각각 읽기 요청 840건이 모두 성공했다. 이 부하가 호출 기록 UI와 메모리에 영향을 주므로 무부하 footprint 점수와 분리했다. 세부 원자료, 실패·제외 표본, 활성화 시간, 원인 분리 한계와 재현 방법은 [M4_WEBVIEW_RECYCLE](M4_WEBVIEW_RECYCLE.md)에 있다.

전용 소스 사본의 workspace bins build 후 설치형 플러그인 경로로 호스트 검사를 실행해 195 passed / 7 ignored였다. clippy -D warnings, 제품 UI 146 passed / 1 skipped, 제품/실험 tsc와 수집기 self-check도 통과했다. 조용한 교체의 native E2E 실패는 그대로 남겼으며 전체 E2E 통과로 보고하지 않는다. [검증 명세](../bench/results-m4-validation.json)와 [산출물 영수증](../bench/results-m4-artifacts.json)을 참조한다.

제품 Rust/TypeScript/Cargo 설정 변경은 0이며 자동 정리 설정과 초안/진행 중 작업 경합 정책도 추가하지 않았다. 영상 기반 깜빡임·무지연 첫 페인트, 실제 사용자 온보딩/설정 복원, 기존 전체 MCP E2E, release·공증·배포는 미검증이다. 사용자 앱·DB·SSH를 조작하거나 공유 release 출력을 변경하지 않았고 commit/push도 하지 않았다.
