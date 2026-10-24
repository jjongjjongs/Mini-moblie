WIE Mobile - 리눅스판 (x86_64)
================================

Android 앱과 같은 에뮬레이터 코어와 같은 게임 로더(LGT 펌웨어, 단말 글꼴
포함)로 KTF / LGT / SKT / BREW / J2ME 게임을 실행한다. 게임 목록, 가상 패드
편집, 세이브 가져오기 같은 앱 화면은 없고, 게임 파일을 지정해 바로 실행한다.

실행
----
    tar xzf wie-linux-x86_64.tar.gz
    cd wie-linux-x86_64
    ./wie_cli 게임.zip        (또는 게임.jar, 게임.jad)

필요한 시스템 라이브러리 (일반 데스크톱 배포판에는 대부분 이미 있음):
    Ubuntu/Debian: sudo apt install libasound2t64 libxkbcommon-x11-0
                   (오래된 배포판은 libasound2)

세이브 데이터
-------------
    ~/.local/share/wie/

키 배치
-------
    1 2 3          숫자 1 2 3
    Q W E          숫자 4 5 6
    A S D          숫자 7 8 9
    Z X C          *  0  #
    방향키          상하좌우
    스페이스        확인(OK)
    왼쪽 Shift      왼쪽 소프트키
    오른쪽 Shift    오른쪽 소프트키
    Backspace      취소(CLEAR)
    F1 / F2        통화 / 종료
    ` / Tab        음량 + / -

창 크기는 마우스로 늘리면 그에 맞춰 확대된다.

로그
----
    RUST_LOG=info ./wie_cli 게임.zip
