# 배포 빌드: 서명·공증 설정과 릴리스 절차

AxDB 설치 파일(macOS `.dmg`, Windows `.exe`)을 만들고, macOS 버전을 Apple 서명과 공증까지 마치는 과정을 정리한 문서입니다. 처음 한 번만 하는 **준비(1~3)**와 릴리스마다 반복하는 **빌드(4~6)**로 나뉩니다.

> 이 문서에는 비밀값을 적지 않습니다. 실제 Key ID, Issuer ID, `.p8` 파일, 비밀번호는 키체인이나 개인 보관소에만 둡니다.

## 왜 필요한가

| 단계 | 하지 않으면 | 하면 |
|---|---|---|
| **서명(Developer ID)** | macOS가 "확인되지 않은 개발자"라며 실행을 막는다 | 누가 만든 앱인지 증명된다 |
| **공증(Notarization)** | 서명이 있어도 "악성 소프트웨어를 확인할 수 없음" 경고가 뜬다 | Apple이 자동 검사를 마친 앱이 되어 더블클릭으로 바로 열린다 |
| **스테이플(staple)** | 처음 열 때 인터넷으로 공증 여부를 확인한다 | 오프라인에서도 경고 없이 열린다 |

공증은 App Store 심사가 아닙니다. 앱이 공개되지 않고 비용도 들지 않으며, 보통 1~5분 걸리는 자동 검사입니다.

## 준비물

- 유료 Apple Developer 계정(개인 팀 또는 회사 팀). 이 프로젝트는 개인 팀 **SangWook Park(Team ID `QRW9X3848W`)**을 쓴다.
- Xcode(인증서 발급용)와 Xcode Command Line Tools(`xcrun notarytool`, `stapler`)
- Rust, Node 22 이상, pnpm. Windows 빌드에는 다음도 필요하다.
  ```sh
  rustup target add aarch64-apple-darwin x86_64-apple-darwin x86_64-pc-windows-msvc
  brew install llvm nsis
  cargo install cargo-xwin --locked
  ```

---

## 1. Developer ID Application 인증서 발급 (Xcode, 최초 1회)

App Store 밖에서 배포하는 앱에 서명하는 인증서입니다. 개발용 **Apple Development** 인증서와 다르니 혼동하지 마세요.

1. **Xcode → Settings…**(`⌘,`)를 엽니다.
2. 사이드바에서 **Apple Accounts**를 고릅니다.
   - 개인 Apple ID가 목록에 없으면 왼쪽 아래 `+` → **Apple ID**로 로그인합니다.
3. 오른쪽에서 배포에 쓸 팀(예: **SangWook Park · Developer Team**)을 선택합니다.
4. 팀 화면 오른쪽 아래의 **Manage Certificates…**를 누릅니다.
5. 인증서 창 왼쪽 아래 **`+ ⌄`**를 누르고 → **Developer ID Application**을 고릅니다.
6. 목록에 **Developer ID Certificates** 항목이 생기면 **Done**을 누릅니다.

확인:
```sh
security find-identity -v -p codesigning | grep "Developer ID Application"
# 예: "Developer ID Application: SangWook Park (QRW9X3848W)"
```

참고 사항:
- 메뉴가 회색이라 만들 수 없으면 계정 **소유자(Account Holder)** 권한이 필요하다. 소유자가 developer.apple.com → Certificates에서 발급해 `.cer`를 전달하면 된다.
- "Not in Keychain"으로 표시된 인증서는 다른 Mac에서 만든 것이라 이 Mac에서는 서명에 쓸 수 없다. 무시해도 된다.
- 인증서와 개인 키는 로그인 키체인에 저장된다. 다른 Mac으로 옮길 때는 키체인 접근에서 `.p12`로 내보낸다(암호 설정).

## 2. App Store Connect API 키 발급 (개발자 사이트, 최초 1회)

공증 서버에 인증할 때 씁니다. Apple ID와 앱 전용 암호로도 인증할 수 있지만 계정 불일치로 401 오류가 나기 쉬워서, **API 키 방식을 표준으로 씁니다**.

1. https://appstoreconnect.apple.com 에 로그인합니다.
2. **사용자 및 액세스(Users and Access) → 통합(Integrations) → App Store Connect API → 팀 키(Team Keys)**로 이동합니다.
3. **`+`(키 생성)**를 누르고 이름 `axdb-notary`, 액세스 권한 **Developer**로 만듭니다.
4. **API 키 다운로드**를 눌러 `AuthKey_<KEY_ID>.p8`를 받습니다. **한 번만** 받을 수 있습니다.
5. 같은 화면의 **Key ID**와 상단의 **Issuer ID**를 확인합니다.

## 3. 공증 자격 증명을 키체인에 저장 (최초 1회)

```sh
xcrun notarytool store-credentials axdb-notary \
  --key ~/Downloads/AuthKey_<KEY_ID>.p8 \
  --key-id <KEY_ID> \
  --issuer <ISSUER_ID>
```

`Success. Credentials validated. Credentials saved to Keychain.`이 나오면 성공입니다. 그 뒤로는 `--keychain-profile axdb-notary`로만 참조하고, 키 값은 다시 입력하지 않습니다.

- 저장이 끝난 `.p8` 파일은 Downloads에 두지 말고 암호 관리자나 안전한 보관소로 옮긴다. 저장소에 커밋하면 안 된다.
- 확인 명령: `xcrun notarytool history --keychain-profile axdb-notary`

---

## 4. 버전 지정 (릴리스마다)

버전은 세 곳이 같아야 합니다.

| 파일 | 키 |
|---|---|
| `Cargo.toml` (workspace) | `version = "0.1.0"` |
| `src-tauri/tauri.conf.json` | `"version": "0.1.0"` |
| `ui/package.json` | `"version": "0.1.0"` |

## 5. macOS: 서명된 universal dmg 빌드 → 공증 → 스테이플

```sh
pnpm --dir ui install --frozen-lockfile
pnpm --dir ui build

# 서명 인증서를 지정해 Apple Silicon + Intel 겸용(universal)으로 빌드
export APPLE_SIGNING_IDENTITY="Developer ID Application: SangWook Park (QRW9X3848W)"
pnpm dlx @tauri-apps/cli@2 build --config src-tauri/tauri.conf.json \
  --target universal-apple-darwin --bundles app,dmg

DMG=target/universal-apple-darwin/release/bundle/dmg/AxDB_0.1.0_universal.dmg

# 서명 확인 (Authority=Developer ID Application, flags=runtime)
codesign -dvv target/universal-apple-darwin/release/bundle/macos/AxDB.app

# 공증 제출 후 결과까지 대기 (보통 1~5분)
xcrun notarytool submit "$DMG" --keychain-profile axdb-notary --wait

# 공증 티켓을 dmg에 첨부
xcrun stapler staple "$DMG"

# 최종 확인
xcrun stapler validate "$DMG"
spctl -a -vv -t open --context context:primary-signature "$DMG"   # source=Notarized Developer ID
```

공증이 `Invalid`로 나오면 원인 로그를 봅니다.
```sh
xcrun notarytool log <SUBMISSION_ID> --keychain-profile axdb-notary
```

## 6. Windows: NSIS 설치 파일(.exe) 빌드

macOS에서 크로스 컴파일합니다.

```sh
export PATH="$(brew --prefix llvm)/bin:$PATH"
pnpm dlx @tauri-apps/cli@2 build --config src-tauri/tauri.conf.json \
  --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
# 결과: target/x86_64-pc-windows-msvc/release/bundle/nsis/AxDB_0.1.0_x64-setup.exe
```

- WebView2 런타임이 없는 PC에서는 설치 중에 자동으로 내려받는다.
- 현재 Windows 설치 파일에는 **서명하지 않는다**. 처음 실행하면 SmartScreen 경고가 뜨니, 사용자는 **추가 정보 → 실행**을 누르면 된다. 경고를 없애려면 별도로 구매한 코드 서명 인증서(OV/EV)를 `bundle > windows > signCommand`에 연결해야 한다.

## 7. 배포

```sh
shasum -a 256 "$DMG" target/x86_64-pc-windows-msvc/release/bundle/nsis/AxDB_0.1.0_x64-setup.exe
gh release create v0.1.0 "$DMG" target/x86_64-pc-windows-msvc/release/bundle/nsis/AxDB_0.1.0_x64-setup.exe \
  --title "AxDB 0.1.0" --notes-file <릴리스 노트>
```

받는 사람은 GitHub Releases 페이지에서 OS에 맞는 파일을 내려받습니다. macOS는 dmg를 열어 AxDB를 응용 프로그램 폴더로 끌어 놓으면 되고, Windows는 setup.exe를 실행하면 됩니다.

---

## 문제 해결

| 증상 | 원인 | 해결 |
|---|---|---|
| `notarytool store-credentials`가 **401 Invalid credentials** | Apple ID와 앱 전용 암호가 맞지 않음. 다른 계정에서 만든 암호이거나, 일반 로그인 암호를 넣었거나, 암호를 `--apple-id` 자리에 넣음 | API 키 방식(2~3단계)을 쓴다 |
| Xcode에서 **Developer ID Application**이 회색 | 계정 소유자 권한이 없음 | 소유자에게 발급을 요청한다 |
| `security find-identity`에 Developer ID가 없음 | 인증서는 있지만 개인 키가 다른 Mac에 있음 | 원래 Mac에서 `.p12`로 내보내 가져오거나 새로 발급한다 |
| 앱이 키체인 접근 허용을 계속 물음 | ad-hoc 서명이라 빌드할 때마다 코드 식별자가 바뀜 | Developer ID로 서명한 빌드를 쓴다 |
| 공증 **Invalid**: hardened runtime | 런타임 옵션 없이 서명함 | Tauri 빌드에 `APPLE_SIGNING_IDENTITY`를 설정하면 hardened runtime으로 서명된다. `codesign -dvv`에서 `flags=0x10000(runtime)`인지 확인한다 |
| 내려받은 dmg가 "손상됨"으로 표시됨 | 공증이나 스테이플이 빠졌거나, 서명 뒤에 파일을 수정함 | 5단계를 처음부터 다시 수행한다(staple은 마지막 단계) |

## 보안 수칙

- 앱 전용 암호나 `.p8` 키를 명령 인자, 채팅, 커밋에 남기지 않는다. 노출됐다면 즉시 폐기하고 다시 발급한다(account.apple.com의 앱 전용 암호, App Store Connect의 API 키 **취소**).
- 공증 자격 증명은 키체인 프로필(`axdb-notary`)로만 참조한다.
- 인증서 개인 키를 담은 `.p12`는 강한 암호를 걸어 보관하고, 저장소에 넣지 않는다.

## 8. 앱 자동 업데이트 서명 키와 통합 릴리스

0.2.0부터 앱 안에서 이후 버전의 릴리스 노트를 확인하고 업데이트를 설치할 수 있습니다. 기존 0.1.x 앱에는 updater가 없으므로 0.2.0은 설치 파일로 한 번 설치해야 합니다. Apple Developer ID 서명과 별개로 Tauri updater 서명을 검증하며, 공개 키는 `src-tauri/tauri.conf.json`에 포함됩니다.

- 개인 키 위치: `~/.tauri/axdb-updater.key`. 개인 키 내용은 출력하거나 저장소에 넣지 않습니다.
- 암호: macOS 키체인 서비스 `axdb-updater`, 계정 `signing-key-password`. 스크립트가 `security find-generic-password`의 출력을 환경 변수로만 받아 사용하고 빌드 뒤 해제합니다. `set -x`를 끄며 암호를 CLI 인자로 전달하지 않습니다.
- **키를 분실하면 기존 설치 앱의 업데이트 체인이 끊깁니다.** 암호와 함께 별도 보안 저장소에 백업하세요. 임의로 새 키를 만들고 공개 키만 교체해도 기존 앱은 새 서명을 신뢰하지 않습니다. 그런 경우 사용자는 새 설치 파일을 수동으로 설치해야 합니다.

세 버전(Cargo workspace, Tauri, UI)을 맞추고 `docs/release-notes/v<버전>.md`를 작성한 뒤 실행합니다. 이미 존재하는 출력 디렉터리는 거부하므로 이전 실패 산출물은 먼저 다른 곳으로 옮겨 검토합니다.

```sh
export APPLE_SIGNING_IDENTITY="Developer ID Application: SangWook Park (QRW9X3848W)"
CARGO_TARGET_DIR=target-u1 scripts/release.sh 0.2.0
# 검토 뒤 실제 GitHub 공개를 하려면 처음부터 --publish를 지정합니다.
# 기존 산출물이 있다면 먼저 다른 곳으로 옮깁니다.
CARGO_TARGET_DIR=target-u1 scripts/release.sh 0.2.0 --publish
```

기본 실행은 업로드하지 않고 `dist/release/<버전>/`에 DMG, universal `.app.tar.gz`와 `.sig`, Windows NSIS `.exe`와 `.sig`, `latest.json`을 만듭니다. macOS 앱과 DMG를 공증하고 staple 검증한 뒤, **staple된 앱을 다시 압축하고 서명**하므로 updater로 받은 앱에도 티켓이 포함됩니다. `latest.json`의 `darwin-aarch64`와 `darwin-x86_64`는 같은 universal 아카이브를 가리킵니다. Windows에는 updater 서명이 있지만 Authenticode 인증서는 아직 없으므로 기존 SmartScreen 제한은 남습니다.

`--publish`가 있을 때만 `gh release create --latest`로 `sangwookp9591/db-tool`의 `v<버전>` 릴리스를 공개합니다. 동일 태그를 덮어쓰지 않습니다. 전제 조건은 위의 개발 도구·Developer ID·`axdb-notary` 키체인 프로필·updater 키·GitHub 인증입니다. 드라이버 산출물(`dist/drivers`)은 읽거나 바꾸지 않습니다. 업데이트 피드는 `https://github.com/sangwookp9591/db-tool/releases/latest/download/latest.json`이며 앱 릴리스가 GitHub의 latest를 유지해야 합니다.

앱의 자동 확인은 시작 10초 뒤 유휴 시점과 이후 24시간 주기이며, `settings.json`의 `updater.auto_check` 기본값은 `true`입니다. `updater.skipped_version`은 자동 배지만 숨기며 수동 확인으로 다시 설치할 수 있습니다. debug 빌드, `AXDB_BENCH`가 설정된 실행, `AXDB_AGENT_BRIDGE=1` 실행에서는 자동 확인을 하지 않습니다. 설치와 재시작은 명시적인 버튼으로 진행하며, Windows 설치 프로그램은 실행 중인 앱을 종료·재실행합니다.

참고: [Tauri updater](https://v2.tauri.app/plugin/updater/), [Tauri process](https://v2.tauri.app/plugin/process/).
