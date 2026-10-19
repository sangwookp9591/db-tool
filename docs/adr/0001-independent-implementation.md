# ADR-0001: 독립 설계 (Tauri 2 + Rust, 다형 결과 계약)

- 상태: 채택 (2026-10-15). "드라이버는 컴파일 시 등록" 부분은 [ADR-0003](0003-installable-drivers.md)이 대체했다.
- 결정자: 코디네이터 (Opus 5.5)

## 배경

요구사항은 관계형·그래프·문서·키-값 DB를 각자의 모양 그대로 보여 주는 가벼운 데스크톱 DB 툴이다. 모든 DB를 SQL과 표 모델에 억지로 맞추지 않아야 하고, 새 DB를 붙일 때 공통 UI를 고치지 않아야 한다.

## 결정

처음부터 독립 설계로 만든다. 핵심 계약은 둘이다.

1. **다형 결과 계약:** 한 결과(`ResultSet`)가 표·그래프·문서·메시지를 동시에 담을 수 있다. UI는 결과에 실제로 들어 있는 뷰만 보여 준다. 모든 값은 타입 태그가 붙은 `Value`이고, 정수·decimal·JSON 원문은 문자열로 보내 정밀도를 지킨다.
2. **드라이버가 정의하는 범용 탐색 트리:** 노드 종류, 아이콘, 동작(`Browse`, `OpenQuery`, `OpenSchemaGraph` 등)을 드라이버가 선언한다. 공통 UI는 드라이버 id로 분기하지 않고 manifest의 `capabilities`, `connection_fields`, `query_language`만 본다.

계약의 세부는 [ARCHITECTURE.md](../ARCHITECTURE.md)에 있다.

## 선택한 스택

| 영역 | 선택 | 이유 |
|---|---|---|
| 셸 | Tauri 2 + Rust | 작은 설치 파일과 낮은 유휴 자원 |
| 프런트엔드 | Preact, CodeMirror 6, 직접 만든 가상 표와 Canvas 그래프 | 가벼운 번들. 그래프 배치 계산만 d3-force(ISC)를 쓴다 |
| 자격 증명 | OS 키체인(`keyring`) | 비밀번호와 토큰을 파일에 남기지 않는다 |
| 취소 | 실행마다 `run_id`와 `CancelToken`, 서버 취소를 지원하는 드라이버는 서버에 취소 요청 | 연결 단위가 아니라 실행 단위로 중단한다 |

## 결과

- 새 DB는 `Driver`/`Session` 트레이트를 구현하는 crate 하나로 붙고, `ui/`와 `crates/dbcore`는 바뀌지 않는다. MongoDB·Redis·SQLite 추가로 이를 확인했다([VERIFICATION.md](../VERIFICATION.md)).
- 계약 타입이 serde 직렬화 가능하므로 드라이버를 프로세스 경계 밖으로 옮길 수 있다. 이 가능성이 ADR-0003의 설치형 드라이버가 되었다.
- 표 가상화와 그래프 캔버스 렌더러를 직접 유지해야 한다.

## 재사용 기록

| 출처 | 라이선스 | 구분 | 내용 |
|---|---|---|---|
| neo4rs 0.9.0-rc.10 | MIT | 코드 재사용(vendored, 패치) | fields 순서 노출 getter 추가; Bolt RUN 헤더와 0행 컬럼 순서 보존 |
| d3-force | ISC | 의존성 | 그래프 배치 계산 |

그 밖의 외부 자료는 [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)에 있다.
