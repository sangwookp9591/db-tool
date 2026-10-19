# E1 파일 내보내기

결과 헤더의 내보내기 버튼에서 현재 불러온 값을 저장한다. 표는 CSV·JSON·Markdown, 문서는 JSON·NDJSON, 그래프는 `nodes`/`edges` JSON을 지원한다. 표의 기본 범위는 필터와 정렬을 적용한 순서이며, 옵션을 해제하면 불러온 원본 순서다. 가상 스크롤의 화면 안 행만으로 제한하지 않는다. 적용 전 편집은 포함하지 않으며, 잘린 결과와 다음 페이지가 있는 결과는 저장 전 대화상자에서 불러온 행 수를 명시한다.

CSV는 UTF-8 BOM과 CRLF, RFC 4180 인용을 사용한다. NULL은 빈 칸, 빈 문자열은 `""`이며 NULL 문자열 옵션을 제공한다. JSON의 Int·Decimal과 원문 JSON 내부 숫자는 문자열로 보존한다. 중복 컬럼 이름으로 JSON 객체 값이 덮이는 경우에는 별칭을 요청하는 오류로 중단한다. CSV는 원문 텍스트를 보존하므로 스프레드시트의 수식 해석을 피하려면 가져오기 때 텍스트 형식을 지정해야 한다는 안내를 표시한다.

ERD는 현재 배치와 카메라를 반영하는 벡터 SVG 및 PNG, 그래프 캔버스는 PNG를 지원한다. 현재 테마의 배경 포함과 투명 배경을 선택한다. 성공 토스트에는 파일명·저장 개수가 나오며, 네이티브 저장에서는 Finder에서 보기 버튼이 저장된 파일만 연다. 브라우저 mock은 Blob 다운로드를 사용한다.

## 저장 권한

공식 `tauri-plugin-dialog` 최신 안정 2.x인 **2.8.3**을 추가했다. Context7에서 [공식 Rust API](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/dialog/src/lib.rs)의 `DialogExt`, `save_file` 콜백, `FilePath::into_path`를 확인했고 `cargo info tauri-plugin-dialog@2`로 버전을 확인했다. JavaScript용 dialog/fs 패키지는 추가하지 않았다.

`export_save_dialog(filename, extension)`가 Rust에서 저장 대화상자를 열고 사용자가 선택한 경로를 정규화해 5분 유효 UUID 토큰에 묶는다. 프런트엔드가 임의 경로를 등록하는 명령은 없다. `export_write(path, token, bytes)`는 정확히 일치하는 경로와 일회용 토큰만 소비하고, 다른 경로·만료·재사용·미승인 요청을 거부한다. 세 명령은 메인 창에 한정된다. 저장 대상 symlink는 거부하며, 같은 폴더의 독점 생성 임시 파일을 flush한 뒤 rename해 기존 파일을 교체한다. Unix 파일 권한은 0600이고 최대 크기는 256 MiB다. `export_reveal(token)`은 성공한 저장 기록의 경로만 사용한다. 기록은 5분 및 최대 16개로 제한된다.

네이티브 Rust API로만 대화상자를 호출하므로 `capabilities/default.json`에 JS dialog 권한이나 파일 시스템 권한을 추가하지 않았다. MCP 도구와 DB 드라이버 계약은 변경하지 않았다. App.tsx와 style.css도 E1에서 수정하지 않았다.

## 변경 파일

E1 소유 파일은 `Cargo.lock`, `src-tauri/Cargo.toml`, `src-tauri/src/export.rs`, `src-tauri/src/lib.rs`의 export 등록 부분, `ui/src/{ExportButton.tsx,export.ts,exportImage.ts,export.css,export.test.ts,Results.tsx,VirtualTable.tsx,GraphView.tsx}`, `ui/src/ui/ToastStack.tsx`의 선택적 액션, 이 보고서와 아래 스크린샷이다. 그 외 공유 작업공간의 변경은 다른 작업자가 소유한다. 시작 시에는 `docs/RELEASE_SIGNING.md`만 untracked였으며 해당 파일은 수정하지 않았다.

## 검증

- `pnpm --dir ui exec tsc --noEmit`: E1 구현 후 여러 차례 통과했으나 마지막 공유 변경에서는 `ui/src/drivers/api.ts:21`의 `mockDriverInvoke` 누락(TS2339)으로 실패했다. E1에서 해당 파일과 `mock.ts`는 수정하지 않았다. 로그: `/tmp/axdb-export-tsc-final.log`.
- 내보내기 직렬화 단위 테스트: **7개 통과**. BOM·CSV 인용·NULL/빈 문자열, 큰 정수·numeric·중첩 JSON, Markdown 이스케이프, 정렬/필터 순서, NDJSON 빈 결과, 그래프 구조, 중복 컬럼 거부, SVG XML 이스케이프/배경을 확인했다.
- `cargo test -p axdb --lib`: **45 passed, 2 ignored**. 내보내기 경로 토큰 테스트에서 미승인·다른 경로·재사용·만료 거부, 정상 저장·교체, symlink 거부와 기존 대상 보존을 확인했다. `AXDB_TEST*` 환경 변수는 없어 외부 테스트 DB를 사용하지 않았다.
- 첫 `pnpm --dir ui test`: 95/96 통과, 기존 BranchedMenu gallery 테스트 1개가 5초 timeout. 이후 공유 변경이 포함된 `pnpm --dir ui test --maxWorkers=2`: 99/102 통과, onboarding 2개와 BranchedMenu sidebar 활성 행 검사 1개 실패. 로그는 `/tmp/axdb-export-ui-test.log`, `/tmp/axdb-export-ui-final.log`이며 코디네이터에게 전달했다. 이 전체 실행 이후 E1에 NDJSON 빈 결과 테스트 1개를 더하고 해당 파일 7개 테스트를 통과했다.
- `cargo clippy --workspace --all-targets -- -D warnings`: 병렬로 추가된 `driver_install.rs`, `plugin_host.rs`의 미해결 crate import, `Registry::unregister` 누락, 타입 추론 오류 등으로 실패(exit 101). E1 `export.rs`의 오류는 보고되지 않았다. 로그: `/tmp/axdb-export-clippy.log`.
- 마지막 `cargo test -p axdb`: 공유 변경의 `axdb_driver_package`·`axdb_driver_protocol` 미해결 import 및 위 설치 모듈 타입 오류로 빌드 실패(exit 101). 앞선 라이브러리 테스트 통과와 구분한다. 로그: `/tmp/axdb-export-cargo-final.log`.
- 실제 mock 브라우저 다운로드: CSV BOM과 필터된 11행, JSON 24행의 Int·Decimal·중첩 JSON 숫자 정확성을 파일을 읽어 확인했다. ERD SVG는 XML 파싱 및 벡터 노드·관계 레이블을 확인했고, ERD PNG는 배경 픽셀 `(246,247,250,255)`와 투명 픽셀 `(0,0,0,0)`을 확인했다. 그래프 JSON은 nodes 100개·edges 99개, 그래프 PNG는 1190×316 이미지를 확인했다. 다운로드 파일은 `/tmp/axdb-export-evidence/`에 둔다.
- 스크린샷: [내보내기 대화상자](design-shots/export-erd-dialog.png).

네이티브 앱 GUI, 실제 save 대화상자·Finder 버튼, 외부 DB와 Windows/Linux 런타임은 실행하지 않았다. 네이티브 GUI 검증은 코디네이터가 수행한다. commit/push는 하지 않았다.

진행 중 다른 작업자의 커밋으로 공유 HEAD가 `b725420`으로 바뀌었고 드라이버 패키지·사이드바·온보딩 파일이 추가로 변경되었다. 최종 통합 검사 실패는 코디네이터에게 위치와 로그를 전달했으며, E1에서 해당 작업자의 파일을 고치지 않았다.
