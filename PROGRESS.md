# 진행 기록 (per-game crash/rendering fixes)

브랜치: `claude/zenonia-audio-integration`

이 문서는 세션 간 맥락 유지를 위한 게임별 작업 기록이다. 각 게임의 근본
원인, 적용한 수정, 남은 과제를 적는다.

## 해결 완료 (기기 확인 또는 렌더링 확인)

### 학교가는 길 (LGT 00023917) — 기기 성공
- 증상: 그래픽이 흩어지고 화면 중심이 안 맞고 겹침, 메인메뉴 글자 안 보임.
- 근본 원인: `ShellComponent`의 무-bounds 생성자가 `Component.w/h`를 1×1
  기본값으로 둔 채 셸을 만들어, 셸이 1×1 화면을 채우려 함.
- 수정: `wie_wipi_java/.../lwc/shell_component.rs` `init_core`에서 display의
  `getWidth/getHeight`를 읽어 `w/h`에 기록.

### 광개토대왕2 — 플레이까지 동작
- 근본 원인: `wec.OEMDevice` 미구현(핸드셋 제공 클래스).
- 수정: `wie_ktf/.../classes/wec/oem_device.rs` 신설(`<init>`,
  `enableSleep(Z)Z`, `getSYSTheme()Lwec/SYSTheme;`).
- 렌더링 관련 질문 답: 갈라진 타이틀은 정상 전환 애니메이션 프레임, 검은
  메인메뉴는 정착 상태로 에셋 실패 없음(의도된 것으로 판단).

### 삼국쟁패 — GAMEVIL 스플래시까지
- 근본 원인: `wec.GatewayIP` 미구현 + 잘못된 객체 인자의 class를 읽을 때 panic.
- 수정: `wec/gateway_ip.rs` 신설(`SetBillcommGWIP(Ljava/lang/String;)Z`),
  `value.rs from_raw`의 배열 판별 robustness.

### 귀신사냥2007 — 메뉴/스토리/캐릭터선택까지
- 근본 원인 1: `wec.SYSTheme` 미구현.
- 근본 원인 2: `System.currentTimeMillis()`가 epoch가 아닌 monotonic(~22ms)을
  반환 → 게임이 `substring(9,10)` 하다 StringIndexOutOfBounds.
- 수정: `wec/sys_theme.rs` 신설, register_class robustness,
  `wie_jvm_support/src/runtime.rs`의 `now()`를 Unix epoch 기준으로
  (`EPOCH_BASE_MILLIS = 1_230_768_000_000`). Date/Calendar/Timer에도 일반적 이득.

## 부분 진전 (relocated module 인프라)

### o2jam (KTF 0103034A) — 로드 크래시 해결, launch 시퀀스까지
- 근본 원인: `client.bin28340`은 relocated module인데 재배치 테이블이 3개
  구간으로만 정렬(끝까지 오름차순 아님)이라 `RelocatedModule::parse`가 일반
  모듈로 오인 → 헤더를 코드로 실행, 즉시 크래시(주소 104).
- 수정 1 (`wie_ktf/src/module.rs`, 커밋 acb9f22): 판별 기준을 "재배치 오프셋이
  끝까지 오름차순"에서 wfeature식 "헤더 bss 워드 == 파일명 N"으로 변경. 정렬
  요구 제거(재배치엔 순서 무관).
- 수정 2 (`wie_ktf/src/runtime/init.rs`, 커밋 afb815d):
  - relocated module 자신의 native 메서드(`startClet`/`paintClet`)는
    `fn_body_native_or_exception_table`가 0이고 `fn_body`가 블록-인자
    엔트리이므로, 이 경우 `fn_body`를 인자 블록(r0)으로 직접 호출.
  - `vm_context+0x3c`에 module interface(MNInterface) 테이블 포인터를 채움
    (모듈의 썽크가 `[fp+0x3c]`로 인터페이스를 부름).
- 남은 과제: `startApp`에서 아직 미구현 `MNInterface` 슬롯 9(0x24), 10(0x28),
  20(0x50), 34(0x88) 호출 → 슬롯 34가 null 반환 후 역참조 크래시(0x10c062).
  문서 없는 모듈 ABI 역분석 필요.

## 미해결 — 깊은 게임 내부 결함 또는 비현실적

- **테일즈판타지 / 테일즈판타지2 / 월드오브드래곤 / 카샨 / 나이트테일즈**:
  게임 자체 AOT 코드의 결함(코루틴 바이트코드 인터프리터가 잘못된 인덱스로
  상수 배열 접근, `startApp`의 AIOOBE/NPE 등). 단순 API 공백 아님.
- **싸이디럭스1 (KTF 0103629C)**: `MC_knlGetDLLInterface("m3dInterf")` 3D
  라이브러리 필요 → 비현실적.
- **폴라폴리2007 (KTF 01037216)**: 게임 타이머 콜백이 `MC_grpFillRect` 후 내부
  연결 구조 null 역참조(주소 72). 깊은 게임 로직.

## 작업 중 (다음)

### 나이트세이버 / KnightSaver (MIDP, 2e7a6c31 → 3.jar)
- 순수 MIDP-1.0 J2ME MIDlet (MANIFEST.MF/DESC.jad).
- 크래시: `NoSuchMethodError mmpp/media/MediaPlayer.setMediaSource:([B)V`.
- 방향: `mmpp.media.MediaPlayer`에 `setMediaSource([B)V` 구현.

### LGTTAXI (LGT 00029288, c7275845 → 00029288.jar)
- 크래시: `net.wie.WieError: Allocation failure at net/wie/CletWrapper.startApp`.
- firmware libc 스텁(mprotect, vsnprintf, __android_log_print,
  pthread_mutex_init, strchr) + `MC_miscBackLight` 이후 할당 실패.
- 방향: 어떤 할당이 실패하는지(과대 크기 요청? 스텁의 잘못된 반환값?) 추적.

## 테스트/커밋 규칙
- 커밋 전 `cargo fmt` + `cargo clippy --workspace` 필수.
- 크레이트별 테스트: `cargo test -p wie_ktf` 등 (ALSA 미가용이라 wie_cli/워크스페이스
  전체 빌드는 피하고 대상 크레이트만).
- KTF 캡처 하니스: `wie_ktf/tests/screen_capture.rs`(`ktf_archive_probe`,
  env `WIE_KTF_ARCHIVE`/`WIE_TICKS`/`WIE_SCRIPT`/`WIE_SHOT`/`WIE_REDRAW_ON_REQUEST`).
- LGT 캡처 하니스: `wie_lgt/tests/screen_capture.rs`(`capture_scripted_archive`,
  env `WIE_ARCHIVE`+`WIE_SCRIPT`).
