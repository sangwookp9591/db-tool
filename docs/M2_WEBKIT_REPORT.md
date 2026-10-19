# M2 WebKit 메모리 조사 결과

**목표 미달이며 제품 코드는 원복했다.** 전체 R2/L1 UI 흐름의 도구 없는 회차는 시작 127.5→탭을 모두 닫고 10초 354.5MiB(+227.0)였다. 10만 행만 표시한 축소 재현은 129.8→131.7MiB로 회수됐지만, 이 결과를 편집기·그래프·10개 탭 전체에 일반화할 수 없다. host Vec에 결과를 보관하고 200행씩 읽은 프로토타입은 WebContent를 줄이는 대신 host를 크게 늘려 앱 전체 활성 footprint가 +104.8MiB 악화됐다. [측정·해석·다음 단계](M2_WEBKIT_MEMORY.md), [원자료 요약](../bench/results-m2-summary.json).

공개 browsing-data 삭제, 2GiB/10초 실제 메모리 할당, 제한된 JS/ArrayBuffer 할당과 하네스 DOM 보관 제거 중 목표를 달성한 방법은 없었다. 압박 후 WebKit Malloc resident 감소는 swapped 증가로 바뀌었고 할당량과 footprint는 유지됐다. warn/critical 알림도 없어 '회수 가능한 캐시' 판정이나 기준 완화는 제안하지 않는다. native heap/vmmap/memgraph와 닫힌 DOM WeakRef는 보유를 확인했지만 JS 클로저와 WebKit opaque-root의 소유 경로까지 확정하지 못했다. 사설 API·Web Inspector·사용자 앱/DB/SSH를 사용하지 않았고 commit/push도 하지 않았다.

실험 커맨드·데이터 저장소 UUID·UI 훅은 모두 제거했고, 재현용 [미적용 패치](../bench/m2-prototype.patch), 측정 하네스와 보고서만 남긴다. 전용 `target-m2`와 별도 UI 출력만 사용했으며 측정 앱은 릴리스 산출물이 아니다. 기존 MongoDB 작업의 `M2_REPORT.md`와 다른 작업자의 `RESOURCE_OPT.md`·MySQL 통합 테스트 변경은 보존했다. 검증 상세와 남은 한계는 아래와 전체 조사 문서에 기록한다.

최종 [검증 기록](../bench/results-m2-validation.json): workspace bins 빌드, host cargo test **194 passed / 7 ignored**, host `clippy --all-targets -D warnings`, tsc, 프로토타입 페이지 경계 검사 1개·참조 탐색 검사 1개와 하네스 self-check가 통과했다. 최신 UI 전체 검사는 **143 passed / 1 failed / 1 skipped**이며 `branched-menu.test.ts:170`의 Archive paging 5초 timeout이다([기존 L1](L1_REPORT.md)에도 기록된 실패). 재시도나 timeout 변경으로 숨기지 않았다. release/native 성능 반복, 실제 warn/critical 회수, 전체 DB·SSH·공급자 검증과 +50MiB 달성은 완료하지 못했다.
