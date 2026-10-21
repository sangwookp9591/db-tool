# Rust Clippy 기준선

2026-10-21 기준. 실행 명령: `CARGO_TARGET_DIR=/private/tmp/claude-501/-Users-psw-Projects-db-tool/60172d03-00b0-48b2-9700-4a93d315a645/scratchpad/target-lint cargo clippy --workspace --all-targets --message-format=json`. Clippy가 종료 코드 0으로 끝났다. 아래 집계는 JSON compiler-message 중 warning만 세었으며 Cargo의 duplicate 요약은 다시 더하지 않았다.

총 **809개 경고**, **15개 lint 종류**. deny lint 진단은 없었다.

## 경고 수 lint별

| Lint | 수 |
|---|---:|
| `clippy::unwrap_used` | 657 |
| `clippy::expect_used` | 71 |
| `clippy::print_stderr` | 45 |
| `clippy::print_stdout` | 15 |
| `clippy::unnecessary_unwrap` | 4 |
| `clippy::assertions_on_constants` | 2 |
| `clippy::collapsible_if` | 2 |
| `clippy::manual_is_multiple_of` | 2 |
| `clippy::too_many_arguments` | 2 |
| `clippy::type_complexity` | 2 |
| `clippy::unnecessary_map_or` | 2 |
| `elided_lifetimes_in_paths` | 2 |
| `clippy::redundant_clone` | 1 |
| `deprecated` | 1 |
| `unused_variables` | 1 |

## 경고 수 crate별

| Crate | 수 |
|---|---:|
| `axdb` | 465 |
| `driver-mysql` | 122 |
| `driver-mongodb` | 73 |
| `driver-postgres` | 58 |
| `driver-sdk` | 26 |
| `driver-neo4j` | 19 |
| `driver-sqlite` | 19 |
| `driver-redis` | 12 |
| `dbcore` | 8 |
| `xtask` | 6 |
| `neo4rs (vendor)` | 1 |

## 상위 15개 lint

| 순위 | Lint | 수 |
|---:|---|---:|
| 1 | `clippy::unwrap_used` | 657 |
| 2 | `clippy::expect_used` | 71 |
| 3 | `clippy::print_stderr` | 45 |
| 4 | `clippy::print_stdout` | 15 |
| 5 | `clippy::unnecessary_unwrap` | 4 |
| 6 | `clippy::assertions_on_constants` | 2 |
| 7 | `clippy::collapsible_if` | 2 |
| 8 | `clippy::manual_is_multiple_of` | 2 |
| 9 | `clippy::too_many_arguments` | 2 |
| 10 | `clippy::type_complexity` | 2 |
| 11 | `clippy::unnecessary_map_or` | 2 |
| 12 | `elided_lifetimes_in_paths` | 2 |
| 13 | `clippy::redundant_clone` | 1 |
| 14 | `deprecated` | 1 |
| 15 | `unused_variables` | 1 |

## await 중 lock 보유

`await_holding_lock` 및 `await_holding_refcell_ref` 진단은 0건이다.

## deny lint 결과

기준선에서 발화한 deny lint는 없다. `correctness`, `await_holding_lock`, `await_holding_refcell_ref`, `dbg_macro`, `todo`, `mem_forget`, `unsafe_op_in_unsafe_fn`, `unused_must_use`는 모두 0건이다.
