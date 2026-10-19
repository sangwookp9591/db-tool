# M3 결과

닫힌 탭을 잡던 SwipeRow의 actions/icon VNode와 spring의 완료 callback 보유를 수정했다. 일반 모션 Chromium의 같은 10만 행·300노드·10탭 시나리오에서 GC 후 JS 힙은 80,896,208→19,132,696 bytes, detached DOM은 6,088→0이다. 원본 소스의 RED 실패와 수정본의 open/close·Map 비움·spring 수명주기 GREEN을 확인했다.

Native 5쌍의 닫고 10초 footprint 중앙값은 374.7→353.6MiB였으며 세 쌍 감소·두 쌍 증가였다. 해당 닫힘 지점 5쌍은 모두 개별/짝 load·PID·footprint·DPR 게이트를 통과했고 10회 모두 앱/자식 종료를 확인했지만, 수정본 5회 모두 `시작 +50MiB` 목표에는 미달했다. JS 보유 버그 해결과 일관된 native RAM 감소는 구분한다.

workspace bins 빌드, 테스트 DB를 지정한 host 194 passed/7 ignored, host all-targets clippy -D warnings, 최신 전체 UI 146 passed/1 skipped, tsc와 집중 browser 검사가 통과했다. 실패한 cold-cache 준비 회차도 보존했고 dependency scanner의 Preact 설정·격리 cache를 보정했다. 상세 경로·크기·전후 표·검증 및 미검증 범위는 [M3_RETAINERS](M3_RETAINERS.md), [아티팩트 영수증](../bench/results-m3-artifacts.json), [native 집계](../bench/results-m3-native-summary.json)에 있다.

Release·공증·사용자 설치 앱 검증과 native 잔여 WebKit Malloc 소유 경로 규명은 수행하지 않았다. 사용자 앱·DB·SSH와 다른 앱 창은 건드리지 않았고 commit/push는 하지 않았다. 제품 변경은 SwipeRow/spring 두 파일이며 기존 untracked MySQL 테스트는 보존했다.
