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
    SELECT+START     MiniMobile 종료

게임 중 (기본값, controls.txt에서 바꿀 수 있음)
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

    SELECT+START 또는 MENU(핫키)   게임 끝내고 목록으로

A와 B가 반대로 느껴지면 minimobile/controls.txt 에서 두 줄을 바꾸세요.
처음 실행할 때 기본값으로 만들어집니다.

파일
----
    minimobile/games/              게임 파일
    minimobile/data/               세이브 (Android 앱과 같은 구조)
    minimobile/controls.txt        버튼 설정
    minimobile/log.txt             마지막 실행 기록
    minimobile/last_game_log.txt   마지막 게임의 에뮬레이터 로그

패드가 인식되지 않으면 log.txt에 "패드로 쓸 수 없는 입력 장치" 줄이
남습니다. 그 기기용 SDL 매핑 한 줄을 minimobile/gamecontrollerdb.txt 에
넣으면 읽어 들입니다.
