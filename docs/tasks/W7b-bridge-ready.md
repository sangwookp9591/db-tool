# W7b-bridge-ready: MCP 브리지 시작 직후 요청 유실 수정

이 작업은 반드시 끝까지 완료한다. 사용자 지시가 skill 지침보다 우선한다.

## 코디네이터 재현 (실제 debug 앱)
- `DBTOOL_AGENT_BRIDGE=1`로 앱을 띄운다.
- `agent-bridge.json`이 생기자마자 `tools/call list_drivers`를 보낸다.
- 결과: **60,278ms 동안 응답이 없다가** `{"error":{"kind":"UserDenied","message":"Agent request denied or approval expired"}}`를 돌려준다.
- 앱이 뜬 뒤 같은 요청을 보내면 72ms에 정상 응답한다.

## 원인 추정
브리지가 웹뷰의 레지스트리 이벤트 리스너가 준비되기 전에 요청을 받는다. 그래서 Tauri 이벤트가 유실되고, 호스트 타임아웃이 UserDenied로 매핑된다.

## Change
1. 웹뷰는 레지스트리 준비가 끝나면(manifest 로드와 리스너 등록 완료) `agent_bridge_ready` command로 호스트에 알린다. 웹뷰가 다시 로드되면 이 신호를 다시 보낸다.
2. 호스트는 다음 중 하나로 처리한다.
   - (a) ready 신호를 받은 뒤에만 `agent-bridge.json`을 쓴다.
   - (b) ready 이전 요청은 큐에 보관했다가 ready가 오면 전달한다. 최대 10초를 넘기면 오류를 돌려준다.
   - (a)와 (b)를 함께 쓰는 것을 권장한다.
3. 오류 분류를 바로잡는다.
   - 호스트 전달 타임아웃이나 웹뷰 미준비는 `Unavailable`(또는 `Timeout`)로 돌려준다.
   - `UserDenied`는 사용자가 실제로 거부했거나 승인 대기가 만료된 경우에만 쓴다.
4. 회귀 테스트:
   - 호스트 단위 테스트: ready 이전 요청이 큐에 들어가고 ready 후 전달되는지, ready가 오지 않을 때 Unavailable이 되는지.
   - 스크립트 `bench/agent-bridge-startup.mjs`: 파일이 생기자마자 `list_drivers`를 호출해 2초 이내에 manifest 배열을 받는지 확인한다. 실제 앱에서 5회 반복한다.

## Constraints
- 파일 소유권: `src-tauri/src/agent_bridge.rs`, `ui/src/agent/*`. `src-tauri/src/lib.rs`와 `ui/src/App.tsx`는 W4도 수정 중이므로 수정 직전에 다시 읽고 작은 diff로 고친다.
- `CARGO_TARGET_DIR=target-w7b`를 쓴다.
- W4가 알리기 전까지 ui/dist를 재빌드하지 않는다. 앱 실증이 필요하면 별도 사본 디렉터리에서 빌드한다.
- 테스트 DB만 쓴다.
- git commit을 하지 않는다.

## Observable acceptance
- cargo test(관련 패키지)와 clippy 통과
- UI tsc와 vitest 통과
- 실제 앱에서 시작 직후 `list_drivers` 5/5가 2초 이내에 응답
- 코디네이터 E2E(`bench/e2e-bridge.mjs`) 21/21. 프로필 준비 방법은 스크립트 주석을 참고한다.
