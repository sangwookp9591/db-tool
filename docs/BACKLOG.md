# 백로그

해야 하지만 아직 하지 않은 일입니다. 착수하면 해당 항목을 지우고 보고서에 결과를 남깁니다.

## 테스트 자동화 (2026-10-20 기록)

- **CI 없음.** `.github/workflows`가 없어 모든 테스트를 로컬에서 수동으로 돌린다. 푸시·PR마다 아래를 돌리는 GitHub Actions를 추가한다.
  - `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
  - `pnpm --dir ui exec tsc --noEmit`, `pnpm --dir ui test`
  - 실제 DB 테스트는 서비스 컨테이너(PostgreSQL, MySQL, MariaDB, MongoDB, Redis, Neo4j)로 띄우고 `AXDB_TEST_*` 환경 변수를 넘긴다. 사용자 DB 주소는 쓰지 않는다.
  - macOS 전용 네이티브 검사(`bench/*_native.*`, WKWebView)와 공증은 CI 범위에서 제외하고 릴리스 체크리스트에 남긴다.
- **테스트가 얇은 크레이트.** `driver-redis`(3개), `driver-sdk`(2개)를 다른 드라이버 수준(스키마 탐색, 실행·취소, 오류 코드, 결과 상한)으로 보강한다.

## 기능

- Redis·Neo4j 결과 편집(0.3.1은 PostgreSQL, MySQL·MariaDB, SQLite, MongoDB만 지원).
- 여러 테이블(JOIN) 결과의 부분 편집, 1,000행을 넘는 결과 편집.
- dmg 안의 앱에 공증 티켓 staple(현재 dmg에만 staple되어 오프라인 첫 실행 시 온라인 확인이 필요).

## 검증 (사용자 계정 필요)

- 실제 Claude·ChatGPT 로그인과 호출, OS 키체인 프롬프트, 클라우드 IAM 실계정, 배포된 업데이트의 실제 설치.
- 메모리 목표: 탭을 모두 닫은 뒤 시작 대비 +50MiB 이내(현재 +72~87MiB, WebKit 내부 할당). 기준 결정 필요.
