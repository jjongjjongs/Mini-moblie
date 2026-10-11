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

### 나이트세이버 / KnightSaver (MIDP, 2e7a6c31 → 3.jar) — 크래시 해결 ✅
- 순수 MIDP-1.0 J2ME MIDlet (MANIFEST.MF/DESC.jad), bare-jar 경로(wie_j2me).
- 크래시: 로고 스플래시의 `MainCanvas.PlaySound([B)V`가
  `NoSuchMethodError mmpp/media/MediaPlayer.setMediaSource:([B)V`.
- 근본 원인: `MediaPlayer`가 location(경로) 기반 클립 로드만 지원. 이 게임은
  jar에서 직접 `.mmf` 바이트를 읽어 `setMediaSource([B)V`로 넘김.
- 수정 (`wie_midp/.../mmpp/media/media_player.rs`, 커밋 48dfa18): `setMediaSource([B)V`
  추가 — 바이트 배열을 저장하고 stale location/handle을 지운 뒤, `start`에서 그
  바이트를 SMAF 경로로 로드(없으면 기존 location 폴백). 파싱 실패는 무해한 no-op.
- 부수: J2ME headless probe 하니스 추가(`wie_j2me/tests/probe.rs`, env `WIE_J2ME_JAR`).
- 결과: 2000틱 무크래시 실행. 기기 확인 대기.

### 아무이유없어 / LGTTAXI (LGT 00029288, c7275845) — 크래시 해결 ✅
- 크래시: `net.wie.WieError: Allocation failure`. 첫 실행은 튜토리얼에서, 재실행은
  스플래시(SN Mobile/GOW ARTS) 다음 화면에서.
- 근본 원인: 게임 자체 렌더러가 `MC_grpCreateImage`로 만든 이미지 플레인(예: 57×58
  RGB565)에 직접 픽셀을 그리는데, 선언한 높이보다 3~4행 더 써서 버퍼 끝을 넘김.
  실기기는 할당 granularity의 여유로 흡수하지만, 우리는 픽셀에 딱 맞게 할당해서
  그 stray store가 바로 뒤 블록의 할당 헤더/canary(예: 0x4016ccb4 위치)를 덮음 →
  힙 체인이 깨지고 몇 할당 뒤 "no free block"으로 죽음. (게스트 w16 픽셀 store 두
  개가 canary를 덮는 것을 watchpoint로 확인.)
- 수정 (`wie_wipi_c/.../graphics/framebuffer.rs`): 이미지/프레임버퍼 픽셀 버퍼를
  `IMAGE_GUARD_ROWS`(64행)만큼 여유 있게 할당(`alloc_with_guard`). 드로잉 코드가
  읽는 논리 크기(`buffer_size`)는 그대로라 렌더링엔 영향 없음. draw surface가 이미
  갖는 `SURFACE_GUARD_ROWS`의 이미지 판.
- 결과: 첫 실행/재실행 모두 8000틱·2049프레임 무크래시. KTF/LGT/wipi_c 전체
  테스트(48/115/453) 통과. 기기 확인 완료(실행됨).

#### 아무이유없어 — 색 글자 블록 (보류, 근본 원인 미확정) ⏸
- 증상: 인트로/허브 대사 화면에서 **특정 색 단어**가 글자당 한 칸씩 solid 색
  사각형(주황 255,101,0 / 빨강 255,0,0 / 파랑)으로 나옴. 검은 글자와 상당수 색
  라벨("CLR:Skip" 등)은 정상. (기기 스샷 및 로컬 재현 일치.)
- 재현: `WIE_ARCHIVE=<c7275845>` + LGT 하니스 `capture_scripted_archive`,
  `WIE_SCRIPT="600:OK,1400:OK,...,8600:OK"` `WIE_TICKS=9500`. 단, 게임이
  **랜덤 로직 + 벽시계 타이머** 기반이라 블록 화면이 매번 뜨진 않음(RNG 의존).
  컴파일된 테스트 바이너리를 직접 실행하면(카고 래핑 없이) 더 잘 재현됨.
- 조사로 **제거된 원인**: (1) pixel-op 미사용(13만+ draw 전수 op=0), (2)
  FillRect/DrawLine은 로딩 바(dst=0x49048004)뿐, (3) 이미지 디코더 정상(tRNS
  팔레트 검증), (4) IMAGE_GUARD 무관. 블록 화면 소스 이미지 100+개 전수 덤프 →
  "고"(주황 글리프)·"CLR:Skip" 등 **전부 올바른 글리프**, solid 블록 소스 없음.
  블록 색(255,101,0)은 **어떤 소스에도 없음** → 합성(blend) 과정의 아티팩트로 좁혀짐.
- **막힌 지점(heisenbug)**: 화면 픽셀 실시간 추적 계측을 넣으면 그 오버헤드가
  타이밍/RNG를 바꿔 블록 화면 자체가 안 뜸. 계측 없이는 범인 draw 특정 불가.
- 다음 시도 후보: 계측 오버헤드 없는 사후-덤프(메모리에 draw 로그 축적 후 문제
  프레임에서만 덤프), 또는 RNG/타이머 시드 고정. 공용 blend 경로는 근거 없이 손대면
  정상 게임 다수 회귀 위험 → 근본 원인 확인 전까지 수정 보류.

### 질주쾌감스케쳐 (LGT 00031347) — 90도 회전 해결 ✅
- 증상: 소울게이트처럼 화면 전체가 90도 돌아감(가로로 그린 걸 세로 패널에 그대로 냄).
- 수정: `wie_backend/src/quirks.rs`의 `sideways()` 퀵을 AID `00031347`에 추가
  (소울게이트 `000323B3`와 동일). present 시 quarter-turn으로 되돌림.
- 검증: 이용약관 화면이 세로(240×320, 글자 옆으로 누움) → 가로(320×240, 정상)로 바로 섬.
- 참고: 아무이유없어(00029288)는 확인 결과 **세로 게임**(타이틀 화면이 정상 세로)이라
  회전 퀵 적용하지 않음. 사용자 스샷의 이용약관만 돌아 보였던 것.

### 액션퍼즐패밀리2 (LGT 00027D2D, 컴투스) — 인증 통과 ✅
- 증상: billing 소켓(`211.115.66.250:15133`) 인증 응답이 없어 대기화면
  렌더 루프(빈 큐 polling)에 머묾.
- 프로토콜: `[u16be 길이(프리픽스 포함)][u16 타입][페이로드]`, 페이로드는 항상
  `0x30`으로 시작. 콜백 `0x2d340`이 길이 2바이트 먼저 읽음. 핸드셰이크는
  프레임 릴레이(프레임마다 승인 필요). 기기 확인: 로그인(type 0, 73B) 승인 →
  단말모델(type 0x14, 58B "Emulator") → … 각 프레임을 승인해야 진행.
- 수정 (`wie_backend/src/billing.rs::lgt_local_apf2_response`): 이 프로토콜의
  모든 프레임(`u16be 길이==크기` && `[2]==0` && `[4]==0x30`)을 **타입 에코 +
  상태 0**으로 승인(`00 08 <type> 00 00 00 00`). 빅엔디안 길이 + 타입 상위바이트
  0으로 리틀엔디안 매처(gamevil/supersoccer)·이노티아 레코드와 분리. 기기 성공.

### EA프로야구2010 (LGT 0002E1D2) — 인증→오프라인 폴백 ✅
- 증상: billing 소켓(`210.222.18.28:20102`) 인증 응답 없어 로딩중 hang.
  20480 버퍼로 벌크 응답을 읽는 구조(콜백 `0x1cc0`).
- 프로토콜: `[u32be 길이(프리픽스 포함)][u8 명령][필드…]`. 로그인=34B 명령 0x10
  (가입자번호·"V1.0.2" TLV + 트레일러). 타이머가 4B `00 00 00 04` 폴 반복.
- 수정 (`lgt_local_ea_baseball_response`): 로그인은 명령 에코+상태 0으로 승인
  (`00 00 00 09 10 00 00 00 00`) → 기기에서 **재전송 없이 수락**(프레이밍 정확).
  이후 서버 데이터 푸시 대기 → 4B 폴에 빈 프레임(`00 00 00 04`)으로 응답.
- 결과: 기기에서 로딩중을 넘어가 "접속 실패" 표시 후 **오프라인 플레이 가능**
  (창세기전3 에피3식 폴백). 실제 벌크 데이터(로스터/계정) 프로토콜은 미구현이나
  게임은 플레이 가능 상태 도달.

### LGT바이러스 / 컴투스 바이러스 게임존 (LGT 00029CAA) — 서버 선-송신 허브, 보류 ⏸
- 구성: JAR 내부에 binary.mod(286KB) + 미니게임 31개(`1.wds`~`31.wds`) +
  `virusktf.bar`(221KB) + `font.bar`. 로컬 콘텐츠(31게임)가 네트워크 게이트
  뒤에 있는 컴투스 "게임존".
- 증상: `211.115.66.250:34133`(APF2와 같은 컴투스 호스트, 다른 포트)로 접속 후
  로딩바만 렌더하며 `MC_netSocketRead(1, …, 5)` 5바이트 레코드를 반복해서 읽음.
- 근본: **서버 선-송신(server-speaks-first) 프로토콜**. 헤드리스 재현 결과
  게임이 접속만 하고 **write를 전혀 안 함** → 서버가 먼저 5바이트 스트림을
  보내야 클라가 읽고(아마 그 뒤 응답). 3회 접속→몇 번 읽기→재접속 반복.
- 왜 어려운가:
  1) 현재 billing 게이트웨이는 **요청-응답**(`billing::response(request)`가 write
     시에만 호출). write가 없으니 응답 훅이 안 걸림 → **접속 시 push 인프라 신설
     필요**.
  2) 5바이트 레코드 포맷을 전부 RE해야 함(게임존 목록/세션 추정).
  3) 실서버가 죽어 **기대 응답이 관측 불가**(기기·캡처 어디에도 서버가 보낼
     내용이 안 나타남) → daebak/EA처럼 클라 요청을 보고 역산할 참조가 없음.
- 결론: 요청-응답 4게임(섯다/미니히어로즈2/APF2/EA)과 성격이 달라 별도 인프라 +
  포맷 RE + 참조 부재. 보류.

### 09대박맞고-왕후의길 (LGT 0002AABE) — 초기 인증 통과 ✅
- 증상: "초기 인증 중.."에서 무한 대기(218.50.3.88:2508 응답 없음).
- 요청(state 8, `0x5bfa8`): 28바이트 `14 00 01 00 00*16 72 00 00 00` + Adler-32(LE).
- 응답: state 8에서는 `[0x760]==8`이라 12바이트 헤더 파싱을 건너뛰고(`0x5cbba`)
  **읽은 12바이트가 곧 응답 전체**. `0x5c74a`가 u16,u16,u32,u32로 읽어
  `0x7a40..`에 저장, `[0x7034]=8`.
- `[0x7a44]`(u32 @4)가 결과 코드: 1=500원 선물, 2/12=300원 동의, 3=SMS 동의,
  999=표시할 것 없음 → 소켓 닫고 타이틀로 진행(`0x3cdce`). 그 외 값은 화면 7에서 재접속 루프.
- 처리: `billing::lgt_local_ensoni_session_response`가 999로 응답. 메뉴 → 게임하기 →
  1화 진입까지 로컬 확인.
- 남은 것: 랭킹샵은 시나리오 1화 이후에만 열림. 다른 상태(0x5bcc0의 `ENS`+`LGT`
  프레임)는 12바이트 헤더(`[5..7]` LE 길이) + 본문 구조이며 아직 미응답.

## 테스트/커밋 규칙
- 커밋 전 `cargo fmt` + `cargo clippy --workspace` 필수.
- 크레이트별 테스트: `cargo test -p wie_ktf` 등 (ALSA 미가용이라 wie_cli/워크스페이스
  전체 빌드는 피하고 대상 크레이트만).
- KTF 캡처 하니스: `wie_ktf/tests/screen_capture.rs`(`ktf_archive_probe`,
  env `WIE_KTF_ARCHIVE`/`WIE_TICKS`/`WIE_SCRIPT`/`WIE_SHOT`/`WIE_REDRAW_ON_REQUEST`).
- LGT 캡처 하니스: `wie_lgt/tests/screen_capture.rs`(`capture_scripted_archive`,
  env `WIE_ARCHIVE`+`WIE_SCRIPT`).
