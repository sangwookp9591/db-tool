# P1-resource-opt: 메모리·CPU 사용량 측정과 최적화

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.
**측정하지 않은 개선을 주장하지 않는다. 효과가 없는 변경은 되돌린다.**

## 사용자 요구
"메모리 소모, 램을 얼마나 쓰는지, CPU 사용률 최적화도 진행. GitHub 등에서 최적화 리소스를 찾아 반영해도 됨."

## 현재 참고치
초기 측정값(부하 게이트 적용 전이라 참고만 한다):
- AxDB 유휴 RSS: 10초 시점 151~167MiB, 60초 시점 123MiB. 앱 프로세스와 WebKit 프로세스의 합이다.
- 유휴 CPU: 0.07~0.10%

## Change
1. **분해 측정.** `bench/` 아래에 스크립트를 둔다.
   - release `.app`을 대상으로 프로세스별 RSS와 `footprint`(phys_footprint)를 잰다: 호스트(Rust), WebContent, Networking, GPU.
   - 측정 시점은 시작 직후, 10초, 60초, 그리고 다음 작업을 마친 뒤의 RSS와 CPU다:
     - 10만 행 쿼리
     - 300노드 그래프
     - 탭 10개
     - 탭을 모두 닫은 직후, 이때는 회수 여부를 본다
   - 각 조건 5회, 측정 직전 load를 기록한다.
2. **리소스 조사.** Tauri, WKWebView, Rust 메모리 최적화 자료를 찾는다. 웹 검색이나 Context7을 쓰고 GitHub 이슈, 공식 문서, 블로그를 본다. 각 기법에 출처 URL과 "적용함/적용 안 함과 이유"를 `docs/RESOURCE_OPT.md`에 남긴다. 후보 예:
   - tokio 워커 스레드 수 제한(`worker_threads` 2~4, 블로킹 풀 상한)
   - 전역 할당자(mimalloc이나 jemalloc을 시스템 할당자와 비교. 이득이 없으면 도입하지 않는다)
   - 드라이버와 세션의 지연 초기화, 미사용 연결 풀 축소(neo4rs max_connections, tokio-postgres 단일 연결)
   - WKWebView 설정: 개발자 도구 비활성화, 캐시 모델, `incognito`나 data store 정책, 백그라운드 탭 throttle
   - UI: 초기 번들 축소, 비활성 탭과 결과의 메모리 해제 정책. 탭을 닫으면 결과 버퍼를 해제하고 그래프 Worker를 종료하며, 오래된 결과는 LRU로 정리한다.
   - Rust 결과 버퍼: 결과를 보낸 뒤 즉시 drop, 컬럼형 청크 재사용
   - release 프로필: `panic = "abort"`, `opt-level`(3, s, z 비교), `codegen-units = 1`, `lto = "fat"`. 바이너리 크기와 RSS 영향을 측정한다.
   - 로그와 이력 쓰기의 버퍼링, 유휴 타이머 제거
3. 효과가 확인된 것만 적용한다. 각 변경마다 전후 수치를 기록한다.
4. 메모리 누수를 점검한다. 같은 쿼리를 50회 반복 실행하고 탭을 열고 닫는 작업을 50회 반복한 뒤의 RSS 추세를 본다. 기울기를 보고한다.

## Constraints
- **W8(비교 측정)이 진행 중이다.** W8이 끝났다는 코디네이터의 알림이 오기 전에는 측정을 하지 않는다. 그 전에는 조사, 코드 분석, 빌드 설정 준비만 한다.
- 측정할 때는 부하 게이트를 적용한다: 1분 load ≤ 8, 최대 10분 대기, 통과와 실패를 구분해 기록한다.
- 테스트 DB만 쓴다. 기존 기능 동작을 바꾸지 않는다.
- UI에서 드라이버 id로 분기하지 않는다.
- `CARGO_TARGET_DIR=target-p1`을 쓴다.
- 동시 작업: M1(Mongo), C1(미리보기: dbcore, 드라이버, 호스트 일부). 공유 파일은 수정 직전에 다시 읽고 작은 diff로 고친다.
- git commit을 하지 않는다. 완료할 때 `--files-modified`를 정확히 적는다.

## Observable acceptance
- `docs/RESOURCE_OPT.md`: 조사 출처, 적용 여부, 전후 표(프로세스별 RSS, footprint, CPU), 누수 점검 결과
- 빌드, 테스트, clippy, vitest 통과. 기존 기능 회귀는 코디네이터 E2E로 확인한다.
