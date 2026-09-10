# LGT AOT 클래스: 메서드 테이블이 없는 클래스의 오버라이드

## 문제

LGT의 AOT 컴파일된 애플리케이션 클래스는 **이름으로 불릴 일이 있을 때만**
메서드 테이블(이름/디스크립터/엔트리)을 싣는다. 난독화된 내부 클래스는 그
테이블이 아예 비어 있고, 그러면 우리 쪽에서 브리지할 메서드가 하나도 없다.

놈3(00015E3D) 로그가 그 모습이다.

```
Bridging application class f extends java/io/InputStream with 0 callable methods
LGT class at 0x14014a8 dispatches through 0x4d865600, 8 of 19 slots its own
...
throwing java exception: java/lang/AbstractMethodError Abstract read()I method called
GameCanvas.loadMenu() : java.lang.AbstractMethodError: Abstract read()I method called
```

`f`는 `java.io.InputStream`을 상속하고 19개 디스패치 슬롯 중 8개가 자기
것인데, 메서드 테이블은 비어 있다. `DataInputStream`이 이 스트림을 감싸고
`read()`를 부르면 추상 `InputStream.read()`로 떨어진다.

`run()V` 하나에 대해서는 이미 같은 처방이 있었다 (`dispatch_run_entry`,
서든어택 포켓의 로더 스레드 `k`). 이번에 그것을 일반화했다.

## 읽는 법

네이티브 링킹은 클래스가 **선언한 슬롯만 채우고 상속하는 슬롯은 0으로 둔다**
(`init.rs`의 vtable 합성이 `entry != 0`을 "자기 것"으로 세는 근거가 이것이다).
그래서:

1. 상속 체인을 위로 걸어 각 조상이 슬롯에 붙인 이름을 모은다.
   애플리케이션 조상은 자기 메서드 테이블에서 (`AppMember::Method`의 `slot`),
   플랫폼 조상은 추출된 메타데이터(`platform_metadata.rs`)에서 가져온다.
   슬롯 번호는 둘을 관통하는 하나의 수열이므로 그대로 겹쳐 쓰면 된다.
   가까운 조상이 이긴다 — 오버라이드와 같은 규칙.
2. 클래스 자신의 디스패치 테이블에서 그 슬롯을 읽는다
   (`metadata + 0x0c`, 길이는 `metadata + 0x26`).
3. 0이 아니고, **애플리케이션 이미지 안을 가리키며**, 클래스의 메서드 테이블에
   이미 그 이름/디스크립터가 없으면 — 브리지할 오버라이드다.

이미지 밖을 가리키는 엔트리는 플랫폼 자신의 코드고, JVM이 이미 갖고 있다.
0인 슬롯은 상속이고, JVM도 같이 상속한다.

놈3의 `f`에서 나온 것:

```
Bridged f.read()I         from dispatch slot 10 @ 0x1365d
Bridged f.read([BII)I     from dispatch slot 12 @ 0x1383d
Bridged f.skip(J)J        from dispatch slot 13 @ 0x139a1
Bridged f.available()I    from dispatch slot 14 @ 0x13c5d
Bridged f.close()V        from dispatch slot 15 @ 0x13e79
Bridged f.mark(I)V        from dispatch slot 16 @ 0x13d0d
Bridged f.reset()V        from dispatch slot 17 @ 0x13d8d
Bridged f.markSupported()Z from dispatch slot 18 @ 0x13e65
```

## 기존 특례와의 순서

`as_proto`는 (1) 메서드 테이블, (2) `dispatch_run`, (3) Seed1 `p.run`,
(4) `Card::paint` 별칭을 먼저 세우고, **그 다음에** 이 오버라이드를 이름/
디스크립터가 겹치지 않을 때만 더한다. 앞의 넷이 내린 결정은 그대로 남으므로
기존 타이틀의 동작은 바뀌지 않는다 — 없던 메서드가 생길 뿐이다.

---

# 컴파일된 코드가 잡을 수 있어야 하는 예외

## 문제

놈3는 세이브 파일을 이렇게 읽는다.

```
0x32338(name):
    0x328f8(name)  ->  경로를 만들고 FileSystem.isFile 을 묻는다
    없으면 0x32780 ->  movs r0, #0     ; null 을 돌려준다
```

부르는 쪽(`0x1ca42`)은 그 null을 **그대로** `new ByteArrayInputStream(...)`에
넘긴다. 실기라면 `NullPointerException`이 나고, 게임이 그것을 잡는다 —
그게 "아직 세이브가 없다" 경로다. 로그의
`GameCanvas.Caller() : 0:3:0:java.lang.NullPointerException`이 그 catch다.

첫 실행에서 `/a`, `/start`, `/nom` 세 번 모두 이 길을 간다.

두 군데가 이걸 막고 있었다.

## (1) 런타임이 예외 대신 패닉했다

`java_runtime`의 `ByteArrayInputStream::<init>`은 곧장 배열 길이를 읽는다.
null 참조는 JVM 안에서 `Option::unwrap()`이 되어 **Rust 패닉** — 게임이
잡으려던 자리에서 프로세스가 죽었다.

`wie_jvm_support`가 `RT_RUSTJAR` 프로토를 넘겨받을 때 이 두 생성자를
null 검사가 앞에 붙은 같은 본문으로 갈아끼운다 (`refuse_a_null_array`).

## (2) 생성자가 던진 예외를 fatal 로 바꿨다

`method_bridge::invoke`의 일반 메서드 경로는 던져진 예외를
`WieError::JavaException`으로 돌려주고, 디스패처가 그것을 컴파일된
세이브포인트 체인(= 컴파일된 `try`/`catch`)으로 태운다. 그런데 **생성자
경로만** `JvmSupport::to_wie_err`로 스택 트레이스를 문자열화한
`FatalError`를 만들었고, fatal은 타이틀을 끝낸다.

```
net.wie.WieError: Compiled r.run failed: Fatal error:
java.lang.NullPointerException: buf is null
	at java/io/ByteArrayInputStream.<init>([B)V
	at r.run()V
```

`thrown_or_fatal`로 두 생성자 경로를 일반 메서드와 같은 길에 올렸다.
핸들을 만들 수 없는 예외만 fatal로 남는다 — 컴파일된 코드가 이름 붙일 수
없는 것은 잡을 수도 없기 때문이다.

## 결과

세 고침이 다 있어야 놈3가 뜬다. 하나씩 보면:

| 상태 | 결과 |
|---|---|
| 셋 다 없음 | `AbstractMethodError` → 배열 null → **패닉** (에뮬레이터 종료) |
| 디스패치 브리지만 | `read()`는 되지만 null 은 그대로 → **패닉** |
| + null 검사 | NPE 는 나지만 fatal 로 바뀜 → `r.run` 스레드 사망, 0 프레임 |
| + 예외 라우팅 | 게임이 NPE 를 잡고 진행 — 타이틀 화면 → 게임플레이 |
