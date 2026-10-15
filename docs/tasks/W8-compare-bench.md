# W8-compare-bench: Tabularis 0.28.0 vs AxDB 동일 조건 비교 측정

이 작업은 반드시 끝까지 완료한다. 합리적인 가정으로 계속 진행하고, 결정한 내용은 마지막 보고에 적는다. 사용자 지시가 skill 지침보다 우선한다.
**측정하지 않은 것을 빠르다고 쓰지 않는다. 불리한 결과도 그대로 기록한다.**

## 대상
- 기준 앱: Tabularis 0.28.0 공식 aarch64 release
  - 경로: `/private/tmp/claude-501/-Users-iron-Project-psw-db-tool/1fe252ea-91ad-4737-8951-7e8f929f6ea0/scratchpad/baseline/tabularis.app`
  - quarantine은 이미 제거됨
- 수정 앱: 이 저장소의 release 빌드(`cargo build -p <앱 crate> --release`, ui는 `pnpm --dir ui build`)
  - 이름 변경(W6) 이후라면 `axdb`, 아니면 `db-tool`
  - 가능하면 `pnpm tauri build`로 `.app` 번들까지 만든다
  - 앱 번들 크기도 비교한다

## 측정 조건
- 같은 머신(Apple M5, macOS 27)에서 측정한다.
- 측정 직전에 다른 cargo, node, 빌드 프로세스가 없는지 `ps`로 확인하고, 로드 평균을 기록한다.
- 두 앱은 번갈아 측정한다(A, B, A, B …). 각 5회 이상이며 중앙값과 최솟값/최댓값을 보고한다.
- 두 앱 모두 빈 설정(첫 실행 상태)과 연결 1개가 있는 상태를 구분해 측정한다.
  - Tabularis의 설정 경로는 소스(`scratchpad/ref/tabularis`)에서 확인한다.
  - 사용자의 기존 설정은 백업 없이 지우지 않는다. Tabularis는 이 머신에서 처음 실행되었으므로 사용자 데이터가 없을 것이나, 그래도 확인한다.

## 측정 항목
1. **시작 시간:** 프로세스 시작부터 첫 창이 화면에 보일 때까지. 도구는 `bench/measure_app.swift`다(CGWindowList 폴링, 이미 검증됨).
   - 보조 지표로 "첫 의미 있는 페인트"를 잰다. 접근성 트리에 앱 고유 텍스트가 나타날 때까지의 시간이며, 예를 들어 AxDB는 "CONNECTIONS", Tabularis는 "Connections"다. Swift AX 폴링 도구를 추가해 측정한다.
2. **유휴 메모리:** 시작 10초 후와 60초 후, 앱 프로세스와 그 앱이 띄운 WebKit 프로세스의 RSS 합계. 가능하면 `footprint` 명령으로 phys_footprint도 함께 잰다.
3. **유휴 CPU:** 시작 후 30~60초 구간의 평균 %CPU(앱과 WebKit 합계).
4. **대량 결과 렌더링:** 테스트 PG(127.0.0.1:55432, testpw, dbtool_test)에서 `SELECT * FROM e2e.big`(25만 행, 2컬럼)과 `SELECT g, md5(g::text), now() FROM generate_series(1,100000) g`를 실행한다.
   - 두 앱 모두 실행 키 입력부터 결과 표에 첫 행이 보일 때까지, 그리고 결과 수 표시가 나타날 때까지의 시간을 같은 외부 방식(AX 폴링)으로 잰다.
   - 결과 표시 후 메모리(RSS)를 잰다.
   - 결과 행 한도 설정이 다르면(Tabularis 기본 페이지 크기 vs AxDB row_limit) 그대로 기록하고, 가능한 한 같은 수의 행을 가져오도록 맞춘 조건과 기본값 조건을 둘 다 잰다.
   - 연결 생성은 GUI(`orca computer`)로 한다. 이 WKWebView에서는 AX set-value가 불안정하니 click 후 type-text를 쓴다. AxDB는 MCP 브리지 또는 `DBTOOL_CONFIG_DIR`/`AXDB_CONFIG_DIR`에 미리 만든 프로필과 키체인을 써도 된다.
5. **스크롤:** 결과 표를 끝까지 스크롤하는 동안 앱이 응답하는지, 대략적인 소요 시간.
   - 정밀한 FPS 측정이 불가능하면 불가능하다고 쓴다. 수치를 만들어 내지 않는다.

## 산출물
- `bench/COMPARE.md`:
  - 환경
  - 방법
  - 표: 항목별 Tabularis / AxDB 중앙값(최소~최대), 차이
  - 원자료 파일 링크
  - 해석: 어떤 차이가 구조(번들 크기, 의존성, 렌더링 방식)에서 왔는지에 대한 추정. 추정임을 명시한다.
  - 측정 한계
- 원자료: `bench/results-compare-*.json`과 `.txt`
- 측정 스크립트: `bench/` 아래에 두고 재실행 방법을 적는다.

## Constraints
- 사용자 데이터 DB(5432, 5439, 32802)에는 접속하지 않는다.
- 측정 중에는 빌드를 하지 않는다.
- 측정 후 띄운 앱은 모두 종료한다.
- git commit을 하지 않는다.
