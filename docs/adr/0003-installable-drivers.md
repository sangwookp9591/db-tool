# ADR-0003: 설치형 드라이버 (외부 프로세스 플러그인)

- 상태: 채택 (2026-10-19)
- 결정자: 사용자(요구), 코디네이터(설계)
- 대체: ADR-0001의 "드라이버는 컴파일 시 등록" 부분

## 배경

사용자 요구: "내장 드라이버 말고, 처음부터 이용자가 원하는 걸 설치해서 사용할 수 있어야 함. 드라이버 설치가 1순위."

지금은 PostgreSQL·Neo4j·SQLite·MongoDB·Redis 드라이버 5종이 앱 바이너리에 컴파일되어 있다. 그래서 새 DB를 쓰려면 앱 전체를 다시 빌드·배포해야 하고, 쓰지 않는 드라이버까지 앱 크기와 의존성 표면에 포함된다.

## 결정

**드라이버를 독립 실행 파일 플러그인으로 분리하고, 앱은 드라이버 없이 배포한다.** 사용자는 드라이버 화면(또는 온보딩)에서 필요한 드라이버만 설치한다.

### 1. 실행 모델

- 드라이버 하나는 실행 파일 하나다. host가 필요할 때 자식 프로세스로 띄우고 **stdio로 통신**한다.
- 드라이버당 프로세스는 하나이고, 그 안에서 세션을 id로 다중화한다. 프로세스가 죽으면 그 드라이버의 세션을 dead로 표시하고, 다음 요청 때 다시 띄운다(크래시 격리).
- host 쪽 `PluginDriver`/`PluginSession`이 기존 `dbcore::Driver`/`Session` 트레이트를 구현해 RPC로 대리 호출한다. 그래서 **UI·권한·미리보기·MCP·AI 계층은 바뀌지 않는다**(UI driver-id 분기 0 불변식 유지).

### 2. 프로토콜 (`axdb-driver-protocol` crate)

- 프레임 형식: 4바이트 길이 접두사 + **MessagePack**(rmp-serde) 본문. dbcore 타입이 이미 serde라서 그대로 직렬화한다. JSON보다 작고 빠르다.
- 메시지: `{id, method, params}` 요청, `{id, result|error}` 응답, `{id, chunk}` 스트림, `{cancel: id}`.
- 메서드: 트레이트와 1:1로 대응한다. `handshake`(프로토콜 버전, manifest 반환), `connect`, `ping`, `server_info`, `list_children`, `browse`, `execute`(결과를 `chunk`로 스트리밍하고 마지막에 `result`), `expand`, `schema_graph`, `preview`, `preview_edits`, `apply_edits`, `close`, `cancel`.
- 버전: `protocol: 1`. 핸드셰이크에서 major 버전이 다르면 거부하고 "드라이버 업데이트 필요"를 안내한다.
- 취소: host가 `{cancel: id}`를 보내면 드라이버 SDK가 해당 요청의 CancelToken을 취소한다(기존 서버 측 취소를 그대로 쓴다).
- 비밀: 비밀번호는 `connect`의 ConnectConfig.secret으로만 전달한다. 드라이버 stderr는 host가 로그로 받되 기존 redaction을 적용한다.

### 3. SDK (`axdb-driver-sdk` crate)

- `axdb_driver_sdk::serve(driver)` 한 줄로 기존 `impl Driver`를 플러그인 실행 파일로 만든다.
  ```rust
  fn main() { axdb_driver_sdk::serve(driver_postgres::PostgresDriver::new()) }
  ```
- 기존 드라이버 5종은 crate를 그대로 두고 `drivers/<id>/main.rs` 바이너리만 추가한다. 기존 실DB 계약 테스트는 in-process 경로와 플러그인 경로 양쪽에서 돈다.
- PLUGIN_GUIDE.md를 "crate 작성 → SDK로 바이너리 → 패키지·서명 → 설치"로 갱신한다.

### 4. 패키지 (`.axdriver`)

- zip 파일 구성:
  - `manifest.json`: id, name, version, protocol, description, paradigm, homepage, license, platforms
  - `bin/<target-triple>/<exe>`: 플랫폼별 실행 파일
  - `manifest.sig`: manifest 서명
- platforms: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`. 각 항목에 파일 경로와 sha256을 적는다.
- 서명: manifest에 **ed25519 서명**을 붙인다. 앱에는 공식 공개 키를 내장한다. 실행 파일 무결성은 manifest에 적힌 sha256으로 보장한다.
- macOS 실행 파일은 Developer ID로 서명·공증하고, Windows는 가능하면 서명한다.

### 5. 레지스트리와 설치

- 레지스트리는 서명된 단일 `index.json`이다(드라이버 목록, 최신 버전, 패키지 URL, 크기, sha256). GitHub Releases에 올린다.
- **앱 시작 시 네트워크 호출은 없다.** 설치된 드라이버는 로컬(앱 데이터 `drivers/<id>/<version>/`)에서만 로드한다.
- 드라이버 화면을 열면 캐시된 목록을 즉시 표시하고, 백그라운드에서 index.json 하나만 ETag로 갱신한다. 드라이버마다 상세를 따로 요청하지 않는다(목록 표시에 원격 왕복 1회 이하).
- 설치 단계: 다운로드(진행률 표시) → sha256 검증 → 서명 검증 → 원자적 압축 해제(임시 폴더 → rename) → handshake로 protocol과 manifest 확인 → 등록.
- 업데이트와 삭제를 지원한다. 같은 드라이버의 이전 버전은 롤백용으로 1개만 보관한다.
- 파일에서 설치: 로컬 `.axdriver`로 설치한다. 서명이 없거나 알 수 없는 키로 서명된 패키지는 "확인되지 않은 드라이버" 경고와 명시적 동의를 받은 뒤에만 설치한다.
- host 커맨드: `drivers_catalog`(캐시 + 갱신), `driver_install(id|path)`(진행률 이벤트 채널), `driver_update`, `driver_remove`, 그리고 `drivers_list`(기존 커맨드, 설치된 것만).

### 6. UX

- 첫 실행 온보딩의 "DB 드라이버" 장면에서 바로 설치한다(여러 개 선택 가능).
- 레일에 "드라이버" 항목을 추가한다. 화면은 설치됨과 사용 가능 탭으로 나누고, 카드마다 패러다임 아이콘, 버전, 크기, 설치·업데이트·삭제 버튼을 둔다.
- 연결을 추가할 때 드라이버가 없으면 "드라이버 설치" 단계로 이어지게 한다. 기존 프로필의 드라이버가 설치되어 있지 않으면 연결 카드에 "드라이버 필요 · 설치" 버튼을 보여 준다.
- MCP `list_drivers`는 설치된 드라이버만 반환한다.

### 7. 개발과 테스트

- 개발 빌드에서는 환경 변수 `AXDB_DRIVER_DIR`로 워크스페이스에서 빌드한 드라이버 바이너리를 로드할 수 있다(debug 전용). E2E와 cargo test는 이 경로를 쓴다.
- `cargo xtask package-drivers`가 크로스 빌드, 패키지, 서명을 맡는다. 서명 개인 키는 키체인이나 환경 변수로만 받고, 저장소에 넣지 않는다.

## 대안과 기각 이유

| 대안 | 기각 이유 |
|---|---|
| 동적 라이브러리(cdylib, libloading) | Rust ABI가 불안정하고 크래시가 앱 전체로 번진다. 플랫폼별 로딩·서명 문제도 있다 |
| WASM 드라이버 | DB 클라이언트(tokio-postgres, mongodb 등)의 소켓과 TLS를 WASI에서 쓰기 어렵다 |
| 내장 유지 + 일부만 플러그인 | 사용자 요구("처음부터 설치")와 맞지 않는다 |

## 성능 예산

- 10만 행 결과 첫 행 표시: in-process 대비 증가 +20% 이하. 같은 하네스로 전후를 측정한다.
- 드라이버 프로세스의 유휴 메모리는 측정해서 보고한다. 유휴 CPU는 0을 유지한다.
- 앱 크기는 드라이버를 빼면 줄어든다(측정해서 보고한다).

## 결과

- 앱은 드라이버 없이 시작하고, 사용자가 원하는 DB만 설치한다.
- 새 DB는 앱 업데이트 없이 패키지로 배포할 수 있다.
- 비용: 프로토콜과 프로세스 관리 코드, 패키지·서명 파이프라인, 드라이버 다운로드 호스팅이 새로 생긴다.
