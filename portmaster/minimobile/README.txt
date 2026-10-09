MiniMobile - 리눅스 게임기용 (PortMaster 포트, aarch64)
=====================================================

Android 앱과 같은 에뮬레이터로 KTF / LGT / SKT / BREW / J2ME 게임을
실행합니다. ArkOS, ROCKNIX, muOS, Knulli, AmberELEC 등 PortMaster를
지원하는 64비트(aarch64) 펌웨어용입니다.

설치
----
1. 압축을 풀면 나오는 MiniMobile.sh 와 minimobile 폴더를 기기 SD카드의
   ports 폴더에 그대로 복사합니다. (ports 폴더 위치는 펌웨어마다 다릅니다.
   ArkOS·ROCKNIX는 roms/ports 입니다.)
2. 게임 파일(.zip, .jar)을 minimobile/games 폴더에 넣습니다.
3. 게임 목록을 새로 고친 뒤 Ports 메뉴에서 MiniMobile을 실행합니다.

버튼
----
게임 목록
    위/아래          게임 고르기
    좌/우, L1/R1     한 페이지씩
    A 또는 START     실행
    Y                설정 (버튼, 배속, 세이브 관리, 게임 삭제)
    SELECT+START     MiniMobile 종료

게임 중 (기본값)
    D패드 / 왼쪽 스틱  방향키
    A                 확인(OK)
    B                 취소(CLEAR)
    X / Y             * / #
    L1, L2            왼쪽 소프트키
    R1, R2            오른쪽 소프트키
    START             확인(OK)

    SELECT를 누른 채로 (숫자 키)
        위 2   아래 8   왼쪽 4   오른쪽 6
        A 5    B 0      X 1      Y 3
        L1 7   R1 9     L2 *     R2 #

    MENU(핫키)        일시정지 메뉴 (배속, 버튼 설정, 프리셋, 게임 끝내기)
    SELECT+START      게임 끝내고 바로 목록으로

버튼 설정
---------
게임 목록에서 Y, 게임 중에는 MENU 버튼으로 엽니다. 메뉴가 열린 동안
게임은 멈춰 있습니다.

    버튼 배치 바꾸기   패드 버튼마다 "그냥"과 "SELECT 누른 채" 키를
                       고릅니다. ◀▶로 칸, A로 바꾸기.
    프리셋             ◀▶로 바로 바꾸고, A로 목록을 엽니다.
                       목록에서 A 불러오기, X 덮어쓰기, Y 지우기(두 번).
                       맨 아래 "+ 지금 배치를 새 프리셋으로"는 고른
                       게임 이름으로 저장합니다.
    이 게임에 프리셋 고정
                       켜 두면 그 게임을 실행할 때마다 그 프리셋으로
                       바뀝니다.
    배속               0.5배~4배. ◀▶로 바꾸고, 게임마다 기억합니다.
    세이브 관리        (게임 목록에서만)
                       내보내기: minimobile/saves 에 "게임 이름 세이브
                         날짜 시각.zip"으로 저장합니다. 매번 새 파일.
                       가져오기: saves 폴더의 세이브 zip을 고릅니다.
                         지금 세이브는 덮어쓰기 전에 "(가져오기 전)"으로
                         자동 보관합니다.
                       세이브 지우기 (되돌릴 수 없음)
                       Android 앱·Windows판의 세이브 zip과 호환됩니다.
    이 게임 삭제       (게임 목록에서만) 게임만, 또는 세이브까지 지웁니다.
                       되돌릴 수 없습니다.

파일
----
    minimobile/games/              게임 파일
    minimobile/data/               세이브 (Android 앱과 같은 구조)
    minimobile/saves/              내보낸 세이브 zip
    minimobile/controls.txt        지금 버튼 설정 (직접 고쳐도 됨)
    minimobile/presets/            프리셋
    minimobile/log.txt             마지막 실행 기록
    minimobile/last_game_log.txt   마지막 게임의 에뮬레이터 로그

패드가 인식되지 않으면 log.txt에 "패드로 쓸 수 없는 입력 장치" 줄이
남습니다. 그 기기용 SDL 매핑 한 줄을 minimobile/gamecontrollerdb.txt 에
넣으면 읽어 들입니다.
