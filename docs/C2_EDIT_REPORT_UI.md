# C2 row edit UI report

## 구현 내용

- `ResultSet.edit_target`, `RowEdit`, `EditRequest`, `EditResult`, `Conflict`와 `edit_preview` / `edit_apply` API command 계약을 추가했다.
- Browse 결과에서 `edit_rows`, `edit_target`, 연결 접근 모드를 모두 확인한 뒤 편집 토글을 표시한다. 편집은 테이블 그리드에서만 가능하며, 기존 행의 키 열은 잠그고 셀 입력·Esc 취소·NULL 지정·행 추가·삭제 표시를 지원한다.
- 변경 내용은 PK 기반 요청으로 만들고, 요청 지문이 현재 편집 상태와 일치하는 미리보기가 성공한 경우에만 확인 대화상자에서 적용할 수 있다. 편집 중 페이지 이동과 새로고침을 막아 대기 중인 변경이 다른 페이지 행에 적용되지 않게 했다. 적용 오류는 변경 상태를 유지한 채 표시한다.
- 기존 C1 미리보기 레이아웃을 `ChangePreviewView`로 추출해 쿼리 미리보기와 행 편집 미리보기가 함께 사용한다. 일반 결과 표는 기존 `ReadOnlyVirtualTable` 렌더 경로를 그대로 사용한다.
- `values.ts`의 순수 변환 함수는 원래 `Value` 태그와 `type_name`을 기준으로 정수·실수·불리언·JSON·날짜/시간·UUID·문자열을 변환하며, 문자열 숫자의 정밀도를 보존한다.
- 브라우저 mock은 명시적으로 MOCK 미리보기 경고를 보여주고 편집/미리보기/적용 명령을 지원한다. mock 변경 후 현재 페이지 재조회에서 수정·추가·삭제 결과가 나타난다.

행 키는 `edit_target.key_columns`에서 받은 기본 키만 사용한다. UI에 물리 식별자 fallback은 없으며, PostgreSQL `ctid`나 SQLite `rowid`를 사용하지 않는다.

## 검증

- `pnpm --dir ui exec tsc --noEmit` 통과.
- `pnpm --dir ui test` 통과: 14개 파일, 68개 테스트. 값 변환과 편집 요청 생성 테스트를 추가했다. 기존 브라우저 갤러리 테스트의 `afterAll` 종료가 기본 10초 제한을 넘겨 teardown timeout을 30초로 맞췄다.
- `pnpm --dir ui build` 통과.
- Chromium에서 브라우저 mock으로 편집 모드를 열고 첫 행을 수정하고, 행 추가 및 행 삭제를 표시한 뒤, 변경 미리보기와 확인 적용을 완료했다. 적용 후 browse 표로 돌아오는 것도 확인했다. 실제 DB 연결은 하지 않았다.

브라우저 캡처:

- [c2-edit-browse.png](design-shots/c2-edit-browse.png) — browse 결과와 편집 진입 전
- [c2-edit-pending.png](design-shots/c2-edit-pending.png) — 수정·추가·삭제 대기 상태
- [c2-edit-preview.png](design-shots/c2-edit-preview.png) — mock 변경 미리보기
- [c2-edit-applied.png](design-shots/c2-edit-applied.png) — 적용 후 browse 결과

## 번들 크기

HEAD 기준 UI를 임시 디렉터리에서 다시 빌드하고, 변경 후에는 별도 scratch 출력 디렉터리로 빌드해 JS/CSS 산출물 전체를 gzip level 9로 합산했다. 기준은 228,811 bytes, 변경 후는 232,328 bytes로 **3,517 bytes 증가**했다. 40 KB gzip 예산 안에 있다.

## 가정과 범위

- 빈 행의 모든 필드는 NULL로 시작한다. 사용자는 컬럼 입력기에서 필요한 값을 지정한다.
- mock은 실제 드라이버 실행을 흉내 내지 않고, 화면 흐름과 편집 후 재조회만 보여주는 데모다.
- 변경 파일은 UI와 이 보고서로 제한했다. 백엔드 계약 및 호스트 구현은 병렬 작업자의 영역이다.
