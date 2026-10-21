# 코드 정책

이 문서는 Rust workspace와 `ui/`의 기본 품질 기준을 정한다. 1단계(설정과 기준선)는 끝났고, 2단계 중 `unwrap`·`expect`·`print` 정리와 deny 전환은 0.3.4에서 끝났다. 나머지 warn lint는 아직 정리하지 않았다(아래 단계별 적용 참고).

## Rust

- workspace lint 설정을 모든 멤버 crate가 상속한다. correctness, `unsafe_op_in_unsafe_fn`, `unused_must_use`, `await_holding_lock`, `await_holding_refcell_ref`, `dbg!`, `todo!`, `mem::forget`, **`unwrap_used`, `expect_used`, `print_stdout`, `print_stderr`**는 deny다. 나머지 성능·관용성 lint는 우선 warn으로 기록한다.
- 테스트 코드에서는 `unwrap`과 `expect`를 허용한다. 제품 코드에서는 오류를 호출자에게 전달하거나 문맥을 더해 처리한다. `clippy.toml`의 `allow-*-in-tests`는 `#[test]` 함수와 `cfg(test)` 모듈에만 적용되므로, `tests/*.rs` 통합 테스트 파일은 맨 위에 `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]`를 둔다.
- 불가피한 panic(상수 URL 파싱, 모든 길이를 받는 HMAC 키 등)만 `#[allow(clippy::expect_used)]`와 `// INVARIANT:` 한 줄 설명으로 허용한다. 새 예외는 리뷰에서 근거를 확인한다.
- 드라이버는 별도 프로세스라 `tracing` 구독자가 없다. 호스트가 드라이버 stderr를 수집해 비밀값을 가린 뒤 기록하므로, 드라이버의 의도된 경고는 고정 문구의 `eprintln!`과 `#[allow(clippy::print_stderr)]`로 남기고 DB 이름·쿼리·오류 내용은 넣지 않는다.
- 빌드 스크립트(`build.rs`)는 `main`이 `Result`를 반환하게 하고, `cargo:` 지시를 위한 `println!`에만 `#[allow(clippy::print_stdout)]`를 둔다.
- 오염된(poisoned) `Mutex`는 패닉하지 않고 `lock().unwrap_or_else(PoisonError::into_inner)`로 복구한다. 보호하는 상태가 일관성을 잃을 수 있으면 복구하지 말고 오류로 반환한다.
- 어떤 동기 Mutex/RefCell guard도 `.await`를 넘겨 보유하지 않는다. 잠금 범위를 작게 하고 await 전에 guard를 해제한다. Clippy는 RefCell과 일부 lock 패턴을 감지하지만 모든 guard 유형을 식별하지는 못하므로 코드 리뷰에서도 확인한다.
- 파일, 프로세스 등 블로킹 작업은 비동기 API를 우선한다. 불가피한 블로킹 I/O나 CPU 작업은 `spawn_blocking`으로 격리한다.
- 불필요한 `clone`, 중간 `collect`, 전체 결과 버퍼링을 피한다. 소유권을 이동하거나 빌리고, 대량 결과는 스트리밍한다. 최적화는 측정으로 확인한다.
- 반환 가능한 오류는 `Result`와 도메인 오류로 표현하고, 오류를 무시하지 않는다. 오류 문구에는 작업 문맥을 포함한다.
- unsafe 사용은 최소화한다. 각 unsafe 블록에는 안전성 전제와 유효 범위를 설명하는 `SAFETY:` 주석을 바로 둔다.

## 성능 기준

CPU와 메모리 사용량을 최우선으로 본다. 타이머 폴링이나 반복적인 busy-wait를 만들지 말고 이벤트/알림 기반으로 대기한다. 큐와 버퍼는 상한을 두며, 큰 파일과 DB 결과는 스트리밍한다. 전체 테이블을 메모리에 올리는 처리를 피하고 페이지네이션 또는 점진 처리를 사용한다. 성능 경로를 바꾸기 전후에는 재현 가능한 벤치마크로 측정한다.

## UI

- 한국어 사용자 화면 문구를 기본으로 한다. 내부 식별자와 프로토콜 문자열은 예외다.
- hover 애니메이션을 추가하지 않는다. 터치와 키보드 입력에서도 핵심 상호작용이 동작해야 한다.
- 선택 UI와 파일 선택 UI는 공유 컴포넌트 `Select`와 `FilePicker`를 사용한다.
- 대규모 목록은 가상화하고, 전체 데이터 렌더링으로 DOM과 메모리를 불필요하게 키우지 않는다.
- 현재 TypeScript는 `strict` 및 `noUncheckedSideEffectImports` 등 엄격 옵션을 켰다. ESLint/Biome은 설치되어 있지 않다. 최소 선택지는 Biome 하나를 도입해 TypeScript 기본 lint/format을 통합하는 것이며, 별도 승인 전까지 의존성은 추가하지 않는다.

## 단계별 적용

1. **1단계 — 설정과 기준선:** lint 설정을 도입하고 기존 경고를 crate별로 기록한다. CI 스크립트는 기본적으로 결과를 보고하며 기존 위반으로 실패하지 않는다.
2. **2단계 — 정리와 강화:** 기준선의 위반을 수정하고 검토한 뒤 warn lint를 단계적으로 deny로 올린다. CI에서 `POLICY_STRICT=1`을 설정해 Clippy를 `-D warnings`로 실행한다.
   - **진행 상황(0.3.4):** `unwrap_used`, `expect_used`, `print_stdout`, `print_stderr`는 정리를 마치고 deny로 올렸다. `cargo clippy --workspace --all-targets`가 이 네 lint에서 오류 없이 끝난다. 프로덕션 코드의 `INVARIANT` 예외는 `lib.rs`(Tauri 빌드 실패), OAuth 상수 URL, HMAC 키, 드라이버의 검증된 편집 컬럼·JSON 슬라이스 직렬화 등 소수이며 각각 근거를 주석으로 남겼다.
   - **남은 일:** 기준선 809건 중 나머지 스타일 경고(first-party 고유 21건: `needless_borrow` 7, `collapsible_if` 3 등)는 아직 warn이다. 이를 정리하면 `POLICY_STRICT=1`을 켤 수 있다. 오염된 `Mutex` 복구 한 줄이 약 108곳에 반복되므로 공통 헬퍼(`lock_or_recover`)로 모으는 정리도 남았다. 실제 기준선 수치는 `CODE_POLICY_BASELINE.md`(정리 전 값)를 참고한다.
