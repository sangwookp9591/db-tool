# M5 작업 보고

유휴 WebView 교체 정책, “유휴 시 메모리 정리” 설정, RAM 상태 복원과 MCP quiesce를 구현했다. 호스트 타이머는 제때 끝났고 짧은 NSProcessInfo activity도 숨김 WebView의 IPC 관측 지연을 없애지 못했으며, 활성화는 호스트의 창 표시와 새 child 포커스로 분리했다.

총 6쌍 중 부하 조건이 맞는 유효 5쌍에서 초안·세션·사이드바는 모두 보존됐고 활성화→2rAF 상한은 92~245ms였다. 그러나 활성화 직후 footprint는 시작 +72.0~+87.0MiB로 +50MiB 목표에 모두 미달하여 **기본 꺼짐을 유지**한다. 대조군보다 메모리는 줄었으나 목표 달성으로 보고하지 않는다.

실제 앱 E2E는 21/21·5/5·13/13, 최종 호스트 단위 검사는 131 passed/4 ignored, 이후 정책 집중 검사는 4 passed였고 clippy·tsc·UI 전체 147 passed/1 skipped도 통과했다. 첫 Rust workspace 전체 305 passed/7 ignored 뒤 후속 전체는 변경하지 않은 Mongo view 스냅숏 오류로 188 passed/1 failed/5 ignored에서 중단됐으며, 실패를 보존하고 전체 green으로 보고하지 않았다. 측정 바이너리와 후속 안전 보강 앱의 영수증은 분리했다.

[상세 결과·제외 조건·재현](M5_WEBVIEW_RECYCLE.md), [6쌍 원자료](../bench/results-m5-paired-combined.json), [유효 5쌍 집계](../bench/results-m5-paired-summary.json), [최종 검증](../bench/results-m5-focused-validation.json), [실제 앱 E2E](../bench/results-m5-e2e.json), [정리](../bench/results-m5-final-cleanup.json)를 참고한다. 실제 활성화 타이밍 경합·동시 설치/백업/가져오기·지속 MCP 부하·키체인·영상 깜빡임·release는 미검증이며, commit/push는 하지 않았다.
