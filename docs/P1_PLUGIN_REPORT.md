# P1 설치형 드라이버 완료 보고

2026-10-19 · [ADR-0003](adr/0003-installable-drivers.md) · `CARGO_TARGET_DIR=target-p1`. protocol 1의 u32 길이 + MessagePack 프레임, SDK `serve(driver)`, 지연 실행·세션 다중화·취소·크래시 재시작을 구현했다. PostgreSQL·Neo4j·SQLite·MongoDB·Redis와 example-kv를 독립 실행 파일로 만들었다. 앱의 일반 의존성에는 실제 DB 드라이버나 클라이언트 라이브러리가 없으며, 설치 관리자의 검증된 로컬 경로와 debug 전용 `AXDB_DRIVER_DIR`에서만 등록한다. 공개 `scan_install_dir`·`read_manifest`·`PluginDriver::load_cancelable`은 P2와 공유한다.

큰 표는 기본 구현이 있는 `Session::execute_stream`과 host sink로 처음 200행, 이후 2,000행씩 기존 UI 채널에 전달한다. 행은 소유권으로 넘기고 메시지·청크 디코딩의 중간 버퍼와 셀별 JSON 태그 객체 생성을 제거했다. host는 큰 표의 전체 행을 다시 모으지 않고, 이력·문맥에는 결과 메타데이터를 보존한다. 그래프·문서·작은 결과는 기존 경로를 유지한다. 선택 필드 `Start.streaming`이 없는 기존 protocol 1 드라이버도 재조립 경로로 지원한다. 기존 DB 드라이버 자체는 결과를 모아서 반환하므로 DB 커서 수준의 스트리밍까지 구현한 것은 아니다.

핸드셰이크와 pipe 쓰기는 3초 제한이며 프로토콜 불일치를 거부한다. DB의 최대 3초 롤백 정리와 전송 유예를 별도로 보장해 취소 응답 전에 연결을 끊던 문제를 수정했다. 설치 탐색 취소는 프로세스 kill/reap을 기다린다. `kill_on_drop`, dead 세션·pending 실패, stderr 비밀값 가림과 긴 줄 폐기, 자식 환경의 서명·AI·테스트 비밀 제외를 적용했다. 프레임 상한은 16 MiB, 요청별 전체 응답 상한은 256 MiB이며 초과를 조용히 잘라내지 않는다.

`cargo test --workspace`는 **156 통과·실패 0·ignored 4**였고, 원문을 재사용한 플러그인 실DB 계약 **29/29**가 포함된다. 최종 호환성 검사를 추가한 집중 실행은 프로토콜 **5/5**, SDK **1/1**이며 `cargo clippy --workspace --all-targets -- -D warnings`도 통과했다. ignored인 수동 성능 검사는 별도로 실행했다. 실제 네이티브 E2E는 브리지 **21/21**, 미리보기 **5/5**, 권한·승인 UI **13/13**을 통과했다. 빈 설치 상태의 실제 앱은 MCP 게시, `list_drivers=[]`, 드라이버 프로세스 없음과 정상 UI를 확인했다. 네이티브 Run으로 **100,000행**을 받은 후 필터로 실제 마지막 값 `100000`도 확인했다. [검증 원장](../bench/results-p1-verification.json), [E2E 기록](../bench/results-p1-e2e.log), [빈 시작](../bench/results-p1-empty-start.json), [네이티브 10만 행](../bench/results-p1-native-large-query.json)에 근거를 남겼다.

같은 10만 행 쿼리를 워밍업 후 7쌍 교차 측정했다. 공유 머신 부하와 저전력 모드가 있으므로 절대값보다 같은 하네스의 상대 비율을 비교했다. UI 행은 실제 `LiveResults → Tauri Channel` 직렬화 경계의 값이며 **브라우저 paint 시간은 포함하지 않는다**.

| 경계 | in-process | 플러그인 | 증가율 |
|---|---:|---:|---:|
| 첫 UI 채널 행, debug | 670.61 ms | 677.37 ms | +1.01% |
| 전체 UI 채널 완료, debug | 1,404.97 ms | 1,568.33 ms | +11.63% |
| 첫 IPC 데이터 청크, release | 196.14 ms | 108.60 ms | -44.63% |
| 전체 execute 반환, release | 196.14 ms | 192.80 ms | -1.70% |

두 채널 지표는 +20% 예산을 충족했다. [채널 측정 원자료](../bench/results-p1-plugin-stream.json)와 [release 전송 원자료](../bench/results-p1-plugin-transport.json)에 모든 쌍을 보존했다. 네이티브 UI의 2.51초 관찰값은 자동화 지연이 섞인 기능 확인값이므로 성능 비교에 사용하지 않았다.

연결된 유휴 상태의 10초 RSS/CPU는 아래와 같다. MongoDB만 CPU 카운터가 0.01초 증가했고, 세션을 모두 닫은 뒤에는 6종 모두 10초 CPU 증가가 **0**이었다. RSS는 physical footprint가 아니다. [연결 유휴](../bench/results-p1-plugin-idle.json)와 [미연결 유휴](../bench/results-p1-plugin-idle-disconnected.json)를 분리했다.

| 드라이버 | 연결 유휴 RSS | 10초 CPU 증가 |
|---|---:|---:|
| PostgreSQL | 8.59 MiB | 0 s |
| Neo4j | 11.84 MiB | 0 s |
| SQLite | 6.36 MiB | 0 s |
| MongoDB | 16.75 MiB | 0.01 s, 약 0.10% |
| Redis | 6.14 MiB | 0 s |
| example-kv | 4.75 MiB | 0 s |

별도 release 반복 조회에서 PG 프로세스는 연결 직후 약 7.03 MiB였으나 10만 행 8회 후 5초 유휴에도 **113.64 MiB RSS**가 남았다. 할당자 재사용·메모리 잔류의 원인을 확정하는 heap 프로파일링은 하지 않았으며 후속 개선 대상이다. 현재 앱 debug 실행 파일은 **61,262,328 bytes**다. 기존 `target-d2/debug/axdb`의 77,701,280 bytes와는 약 21.16% 작지만, 다른 로컬 소스 스냅샷이므로 순수 드라이버 분리 효과나 동일 revision의 release 번들 전후 비교로 해석하지 않는다.

검증은 PG `55432`, Neo4j `57687`, MongoDB `57017`, Redis `56379`와 임시 SQLite만 사용했다. `/Applications/AxDB.app`과 사용자 DB에는 접근하지 않았고, 테스트 앱과 자식 6개는 종료를 확인했다. E2E 인증은 기존 debug 비밀 저장소를 사용했으며 실제 키체인·실 AI·실 TLS·다른 OS·공식 패키지 배포/서명 검증으로 확대 해석하지 않는다. P2 패키지 HTTP 검사는 이 실행에서 ignored이며 [P2 보고서](P2_PACKAGE_REPORT.md) 범위다. 시작 시 수정된 tracked 파일 10개와 untracked 루트 9개를 보존했고, 종료 공유 작업 트리 스냅샷은 [검증 원장](../bench/results-p1-verification.json)에 기록했다. commit/push는 하지 않았다.
