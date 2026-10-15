# W4b-graph-panzoom: 그래프 팬·줌 프레임 회귀 수정

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.
측정하지 않은 개선을 주장하지 않는다.

## 배경
`bench/VIEWER_PERF.md`의 W4 측정(Chromium, 같은 조건에서 전후 교대 실행)에서 그래프 팬·줌 프레임 시간이 나빠졌다.

| 노드 | p95 이전 | p95 이후 |
|---|---:|---:|
| 300 | 16.7 | 33.3 |
| 1,000 | 16.7 | 50 |
| 5,000 | 33 | 167~200 |

사용자 요구는 "뷰어가 빠르고, 흔들림 없이 부하 없이 자연스럽게"다. W4가 달성한 다음 결과는 반드시 유지한다:
- 수렴 후 이동 0
- 유휴 렌더 0
- CLS 0
- 유휴 CPU 1% 미만

## Change
1. 프로파일링으로 원인을 먼저 찾는다. Chromium trace나 `performance.mark`로 팬 1프레임의 비용을 쪼갠다. 후보는 다음과 같다:
   - 곡선 간선 경로 계산
   - 라벨이나 텍스트 측정
   - 쿼드트리를 매 프레임 재구축
   - 색상별 일괄 그리기가 깨짐
   - devicePixelRatio 전체 다시 그리기
2. 아래 기법 중 측정으로 효과가 확인된 것만 적용한다.
   - **팬·줌 중 비트맵 변환:** 상호작용 중에는 마지막으로 완성한 프레임(OffscreenCanvas 또는 ImageBitmap)을 transform만 바꿔 그리고, 상호작용이 끝나면(예: 120ms idle) 한 번만 정밀하게 다시 그린다. 이때 흔들림이나 깜박임이 없어야 한다.
   - 곡선과 직선 간선 경로를 Path2D로 캐시한다. 좌표가 바뀔 때만 다시 만든다.
   - 쿼드트리는 좌표가 바뀔 때만 다시 만들고, 팬·줌 때는 다시 만들지 않는다.
   - LOD: 줌 배율이나 노드 수가 기준을 넘으면 라벨, 화살표, 곡선을 생략하고 직선 일괄 그리기만 한다.
   - 화면 밖 culling(뷰포트 AABB).
   - 라벨 텍스트 폭을 캐시한다.
3. 목표(Chromium, VIEWER_PERF와 같은 방법):
   - 300노드 팬·줌 p95 ≤ 16.8ms
   - 1,000노드 p95 ≤ 20ms
   - 5,000노드 p95 ≤ 50ms
   - 첫 표시, 속성 클릭, expand 병합은 나빠지지 않아야 한다
   - 미달이면 미달이라고 쓴다.
4. 네이티브 확인: WKWebView에서 300/1,000노드 팬·줌을 측정한다. W4의 DBTOOL_BENCH 경로에 시나리오를 추가한다.
   - 측정이 불가능하면 불가능하다고 쓴다.
   - 측정할 때 load average를 기록한다.

## Constraints
- 파일 범위: `ui/src/GraphView.tsx`, `ui/src/graph.ts`, `ui/src/perf/*`, `ui/src/workers/graph.worker.ts`, `bench/*viewer*`, `bench/VIEWER_PERF.md`(결과 절 추가)
- UI에서 드라이버 id로 분기하지 않는다(grep 0).
- 테스트 DB만 쓴다.
- `CARGO_TARGET_DIR=target-w4b`를 쓴다.
- git commit을 하지 않는다.

## Observable acceptance
- UI tsc, vitest, build 통과
- `bench/VIEWER_PERF.md`에 "W4b 팬·줌" 절을 추가한다: 전후 교대 실행 표, 프로파일 원인, 적용한 기법과 되돌린 기법
- 흔들림, 유휴, CLS 지표의 재측정값이 W4와 같아야 한다(0 유지)
