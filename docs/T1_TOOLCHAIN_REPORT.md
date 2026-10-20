# T1: 도구 체인 최신화

2026-10-20, 기준 계약: [ADR-0004](adr/0004-ax-native-workbench.md), [ADR-0003](adr/0003-installable-drivers.md), [아키텍처](ARCHITECTURE.md). 시작 시 `git status --short`는 비어 있었다. commit/push, 사용자 DB/SSH, `/Applications/AxDB.app`과 GUI 조작은 하지 않았다. 빌드는 `CARGO_TARGET_DIR=target-t1`, `CARGO_BUILD_JOBS=6`으로 격리했고 UI 산출물은 `/tmp/axdb-t1-ui-{before,after}`에 생성했다.

## 현황과 적용

UI는 의도적으로 Preact 11.0.1을 사용한다. React 19로 교체하지 않았다. TypeScript 7.0.2와 플랫폼별 네이티브 컴파일러 optional dependency가 설치되어 있다. `tsc -v`도 7.0.2다. 모든 신기능을 무조건 쓰는 것을 완료 기준으로 삼지 않고, 기존 안전 계약과 런타임 호환성을 유지하는 설정을 적용했다.

`verbatimModuleSyntax`, `erasableSyntaxOnly`, `noUncheckedSideEffectImports`, `noImplicitOverride`, `noFallthroughCasesInSwitch`를 켰다. 기존 Worker 기반 테스트의 `terminate()`에 `override`를 명시한 것 외에는 타입 오류 수정이 필요하지 않았다. 존재하지 않는 `vite.config.ts`를 include에서 제거했다. 실제 설정은 JavaScript인 `vite.config.mjs`이며 이 변경은 해당 파일의 타입 검사 추가를 의미하지 않는다.

Rust는 프로젝트 로컬 `rust-toolchain.toml`에 1.99.1과 rustfmt/clippy, macOS arm64/x64 및 Windows x64 타깃을 고정했다. rustup은 해당 배포를 2026-10-15 배포본, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`으로 확인했다. 전역 default 1.94.0은 변경하지 않았다. workspace MSRV는 1.99이며 각 package가 이를 상속한다. `vendor/neo4rs`는 edition 2021/MSRV 1.81을 유지한다.

## 검토 후 보류

- `exactOptionalPropertyTypes`: 독립 검사에서 35개 파일, 68개 진단. Preact props와 탭/초안/선택 필드에서 명시적 undefined와 생략의 계약 정리가 필요하여 병렬 기능 변경과 섞지 않았다.
- `noUncheckedIndexedAccess`: 독립 검사에서 55개 파일, 350개 진단. 표/그래프/파서의 인덱스 경계를 검증하는 별도 변경이 필요하다. 일괄 non-null assertion으로 숨기지 않았다.
- ES2023 target/lib: ES2022 유지. `bundle.macOS.minimumSystemVersion`이 없고 Tauri는 시스템 WKWebView를 사용하므로 지원 macOS/WebView 최소 버전 없이 ES2023 API 지원을 약속할 수 없다. Windows WebView2 설치는 오래된 macOS 엔진 지원을 증명하지 않는다. 기존 ES2022의 전체 최소 macOS 호환성도 이번 작업에서 새로 입증한 것은 아니다. 근거: [Tauri macOS 기본 최소 버전](https://v2.tauri.app/distribute/macos-application-bundle/), [OS별 WebView](https://v2.tauri.app/reference/webview-versions/).

## 의존성

`pnpm outdated`에서 발견한 호환 업데이트 3개를 반영했다: CodeMirror view 6.43.14→6.43.15, lucide-preact 1.57.0→1.59.0, Vitest 5.0.4→5.0.5. UI 메이저 업데이트는 없었다.

`cargo update --dry-run` 확인 후 호환 범위의 lockfile 업데이트 15개를 적용했다: cc 1.8→1.9, combine 4.6.8→4.6.9, defmt/defmt-macros 1.1.1→1.2, defmt-parser 1.0→1.1, find-msvc-tools 0.1.14→0.1.15, inout 0.2.4→0.2.5, libc 0.2.190→0.2.191, plist 1.10.1→1.10.2, schannel 0.1.29→0.1.30, tauri-winres 0.3.6→0.3.7, tokio 1.53.3→1.54, unicode-normalization 0.1.25→0.1.26, webpki-root-certs/webpki-roots 1.0.9→1.0.10.

호환성을 깨는 업데이트는 보류했다(0.x의 minor도 SemVer상 breaking): base64 0.22→0.23, calamine 0.36→0.37, ed25519-dalek 2→3, getrandom 0.3→0.4, hmac 0.12→0.13, keyring 3→4, sha1 0.10→0.11, sha2 0.10→0.11, zip 4→9, zstd 0.13→0.14. zip 9는 다른 dependency 경로에 이미 공존하지만 host의 zip 4 API 변경은 하지 않았다. prerelease rsa/pkcs1과 dependency constraint가 유지한 일부 transitive patch도 강제 override하지 않았다. 이는 모든 crate의 최신 메이저를 사용한다는 주장이 아니다.

## TypeScript 시간과 UI 크기

초기 `tsc --noEmit` wall time 1.02초. Rust cold compilation과 동시 수행한 변경 후 첫 검사는 5.79초였다. 같은 환경에서 3회 추가 측정한 새 옵션은 3.214/4.916/3.199초, 옵션을 CLI로 끈 비교군은 3.174/4.006/3.586초였다. 시작 전부터 TS7이었으므로 이 수치로 TS6 대비 네이티브 컴파일러 가속 효과를 주장할 수 없고 동시 CPU 부하도 있었다.

Vite 전후 모두 성공. JS 114개 합계 raw 851,071→851,613 bytes(+542), 각 파일 gzip 합계 324,110→324,291 bytes(+181). Vite build 시간 434ms→1.27초는 동시 Rust 빌드 부하가 섞여 성능 회귀의 근거가 아니다. 타입 옵션은 런타임 코드를 추가하지 않으며 증가는 호환 의존성 업데이트와 함께 측정한 값이다.

## 검증 진행

UI 전체: 38 files passed/1 skipped, 147 tests passed/1 skipped. 추가 옵션 적용 후 `tsc --noEmit` 통과. Rust edition 사전 migration 컴파일은 통과(3분 07초). 전체 바이너리/실DB 테스트/clippy는 별도로 아래에 기록한다.

## Rust 2024 검토

`cargo fix --edition --workspace --all-targets --allow-dirty`를 Rust 1.99.1에서 먼저 실행한 뒤 workspace edition을 2024로 올렸다. MSRV에 맞춘 dependency 선택을 위해 resolver도 3을 적용했다. `cargo metadata --no-deps`로 13개 package 전부 edition 2024/MSRV 1.99 상속을 확인했다. Linux 패키지 빌드의 하드코딩된 `rust:1.94` 컨테이너도 `rust:1.99.1`로 맞췄다.

FFI 선언은 SDK, 앱 main, streaming memory 테스트, MySQL 패키지 테스트 4곳에서 `unsafe extern`으로 명시했다. 기존 호출의 unsafe 블록은 유지했다. `std::env::set_var/remove_var`가 새로 unsafe가 되는 Anthropic CLI 테스트는 unsafe로 감싸지 않고, mutex로 보호한 테스트 전용 경로 주입으로 교체했다. 운영 환경변수 읽기는 유지했다. 기존 로그인 상호작용 테스트가 이 주입 경로도 함께 검증한다.

사전 migration의 `tail_expr_drop_order` 경고 위치를 검토했다. PG의 상태 mutex는 이름 있는 `_guard`, Neo4j의 명시적 tx/txn과 전용 graph는 이름 있는 소유자로 유지되며 await가 완료된 다음 임시 future가 먼저 drop되어도 ROLLBACK/COMMIT을 담당하는 객체는 사라지지 않는다. PG FETCH/row next, Neo4j stream.next, MySQL result.stream/next는 빌린 stream/transaction 위의 완료된 future이고, 명시적 drain/rollback/cancel 경로를 유지한다. Mongo restore의 이름 있는 target 세션도 호출 완료까지 유지한다.

호스트에서는 SSH의 Pending 정리 guard, Tunnel/session Arc, RunGuard, plugin Process/Probe, keyring Entry와 관련 경고를 확인했다. 임시 future의 수명 단축이 명시적 guard나 반환 stream/session의 소유권을 대신하지 않으며, 취소·정리 guard는 작업 완료 뒤 drop된다. `recycle::focus`의 if-let→match 자동 변환은 검토 후 되돌렸다. else에는 `None`만 있어 Webview destructor가 없고, Some 경로는 양 edition 모두 해당 분기를 끝낸 뒤 해제하므로 보존용 match가 필요하지 않다.

RPIT는 `file_logger`의 소유 File과 테스트 HTTP handler의 소유 State/HeaderMap이며 추가 참조 수명 캡처 변경이 필요하지 않았다. 예약어 `gen` 식별자, never fallback, unsafe 함수 내부 연산 등은 migration 컴파일이 제안한 추가 수정이 없었다. 이는 가능한 모든 동시 실행 interleaving의 증명은 아니며 아래 실DB 취소·롤백·플러그인 계약 테스트로 확인 범위를 보완한다. 근거: [Rust 2024 migration](https://doc.rust-lang.org/edition-guide/rust-2024/index.html), [if-let 수명](https://doc.rust-lang.org/edition-guide/rust-2024/temporary-if-let-scope.html), [꼬리 식 drop 순서](https://doc.rust-lang.org/edition-guide/rust-2024/temporary-tail-expr-scope.html), [RPIT 캡처](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html), [새 unsafe 환경변수 API](https://doc.rust-lang.org/edition-guide/rust-2024/newly-unsafe-functions.html).

추가 확인: React/ReactDOM 직접 import와 forwardRef 호출은 UI 소스에서 발견되지 않았다. style의 숫자 0은 단위가 불필요하며 실제 px 치수는 문자열로 전달한다. Preact 11을 React 19 API의 완전 구현으로 주장하지 않는다.

## Clippy 적용과 한계

초기 `cargo clippy --workspace --all-targets -- -D warnings`는 edition 2024에서 활성화되는 `collapsible_if` 경고로 실패했다. `cargo clippy --fix --workspace --all-targets --allow-dirty -- -D warnings`의 최소 제안을 적용하고 stderr 읽기 루프 1곳을 수동으로 `while let`으로 바꿨다. 변경 diff의 모든 조건은 기존 평가 순서·단락 평가를 유지한다. 권한/미리보기 검증, symlink 거부, SSH 지문 비교와 secret 제거는 그대로 실행된다. 새로운 let-chain에서 잠금이 포함된 agent bridge·assistant 경로는 성공한 본문까지 guard를 유지하고 실패 시에는 기존처럼 본문 밖에서 해제된다.

표준 `as_chunks::<2>()` 변환은 기존 `chunks_exact(2)`의 남은 원소 처리와 같으며 Redis의 짝수 길이 검증을 유지한다. SQLite snapshot의 불필요한 `let rows = …; rows` 제거는 2024의 꼬리 식 임시값 drop 규칙하에서 statement보다 query iterator가 먼저 정리되는 구조다. 코드 전체의 포맷이나 기능 확장은 하지 않았다.

수정 후 strict Clippy 전체 타깃 검사는 exit 0으로 통과(24.37초). 제외된 로컬 벤더 dependency `neo4rs`에는 `AtomicUsize::fetch_update`가 1.99에서 `try_update`로 이름이 바뀐 deprecation 경고 1개가 남는다. workspace 코드의 경고 허용 attribute를 추가하지 않았으며, 벤더 edition 2021/MSRV 1.81을 유지하기 위해 새 API 치환을 하지 않았다. 따라서 '전체 dependency 출력까지 warning 0'이라는 의미의 통과는 아니다.

`rustup check`도 현재 stable 최신을 1.99.1로 확인했다. 설치한 x86_64 macOS/Windows 표준 라이브러리는 크로스 빌드나 설치 실행을 검증한 근거가 아니며 이번에는 native macOS arm64 검사를 수행했다.

설치 과정에서 rustup 자체는 1.29.0→1.29.1로 자동 갱신됐으며, 전역 default toolchain은 계속 1.94.0이다. `rustup show`의 active 이유는 프로젝트 `rust-toolchain.toml` override다.


최종 스트리밍 검사는 payload 166,193,029 bytes에서 기존 64MiB 증가 한도를 통과했다. host peak RSS는 작은 백업 30,670,848→큰 백업 31,309,824 bytes(+638,976), driver peak RSS는 17,285,120→19,824,640 bytes(+2,539,520)였다. [최종 원자료](../bench/results-t1-streaming-plugin.json)는 T1 이름으로 보관하고, 테스트가 덮어쓰는 기존 `bench/results-w4b-streaming-plugin.json`은 시작 버전으로 복구했다. 이 수치는 debug 테스트 harness 측정이며 실제 사용자 앱 footprint나 release 성능의 증거가 아니다.

## 실행하지 않은 검증

x86_64 macOS·Windows·Linux Docker 크로스 빌드/패키지 실행, release 빌드·서명·공증·배포, 실제 앱 GUI/E2E, OS 키체인, live AI 공급자 요청, 실제 SSH/IAM 및 별도 TLS fixture 경로는 실행하지 않았다. TLS 환경 변수가 없는 일부 테스트 함수는 harness상 pass라도 해당 실연결 부분을 조기에 반환하므로 TLS 검증으로 집계하지 않는다. 기본 제외 테스트 7개는 live Anthropic 1개, OS 키체인 1개, 수동 설치/리소스 fixture 2개, 수동 성능 측정 3개다. UI의 기존 제외 테스트 1개도 실행하지 않았다.


## 최종 결과

최종 소스와 다시 빌드한 플러그인 기준 `cargo build --workspace --bins` 통과(20.46초), `cargo clippy --workspace --all-targets -- -D warnings` 통과(10.65초), `cargo test --workspace -- --test-threads=1` 통과(231.86초): 307 passed / 0 failed / 7 ignored. PostgreSQL·Neo4j·SQLite·MongoDB·Redis·MySQL 및 MariaDB의 테스트 endpoint와 `AXDB_DRIVER_DIR=$PWD/target-t1/debug`를 사용했다. 이 집계에는 in-process 드라이버와 40개 plugin contract가 모두 포함된다. 최초 검사도 307 passed/7 ignored였으며, 이후 Clippy 수정 때문에 최종 gate를 다시 실행한 것이므로 실패를 재시도로 지운 결과가 아니다.

UI 전체 147 passed/1 skipped, TypeScript 7.0.2 `tsc --noEmit`, Vite 전후 빌드, `git diff --check`도 통과했다. 후속 `pnpm outdated`는 항목 없이 exit 0이었다. 새로 주석/TODO/미구현 분기를 추가해 기능을 완료한 것으로 표시하지 않았다. AI Operation·capability·커맨드 계약과 승인 경계는 변경하지 않았다.

작업 시작은 clean, 종료는 tracked 수정 54개와 신규 3개다. 신규 파일은 `rust-toolchain.toml`, 이 보고서와 T1 스트리밍 원자료이며, 수정은 도구 체인·package MSRV·호환 lockfile·컴파일러가 지적한 표현·관련 테스트/README 범위다. `vendor/neo4rs` 및 릴리스의 `dist/`, `ui/dist/`, `target`, `target-rel`을 수정하지 않았다. 검사 바이너리는 `target-t1/debug`에 남겼다.
