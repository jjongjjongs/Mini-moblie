//! 오즈 천공의 기사단 (`0103CD8A`): the 코단 world's dungeons fought by the
//! title's own rules.
//!
//! The title plays offline as well as online, and nearly every rule of a fight
//! asks which it is: `network/b.a()B` answers -1 offline, 0 in an area the
//! server runs and 1 in one the title runs. Offline the title walks and swings
//! the monsters itself, lands the player's blows and shots, takes the player's
//! health, gives the experience and the things a fallen monster leaves, and
//! lays the player down; in an area the server runs it waits for the server to
//! say each of those, and in one the title runs - a town - there is nothing to
//! fight.
//!
//! The server that ran the 코단 world's dungeons is gone, and the rules it
//! played by went with it. So a dungeon is fought as offline. The area's kind,
//! the byte the server answers an entry with (`network/a.b(I[BI)V`), is kept
//! in `network/b.Z` as it came instead of as 1, and the emulated server enters
//! a dungeon with kind -1. `b.a()B` then answers -1 there, but only to the
//! fighting: the characters (`game/chars/*`) and the skills
//! (`game/engine/h`). Everything else - loading the map, the player's own
//! record, the screens - asks the same question to choose between the save on
//! the handset and the server's, and keeps being told 1, a town's answer.
//!
//! Two things the server did around a fight are done here as well. It told the
//! title the player had fallen, which opened 부활대기중 (rise at the entrance,
//! or give the dungeon up); and the player it stood up at the entrance had
//! health again.
//!
//! And the server kept what the player held. What a dungeon fought here
//! leaves, and what is worn, used and thrown away in a town, the emulated
//! server never hears of; so before each request the title sends, what the
//! player holds and its gold are read off the title's own belongings and
//! written where that server takes them from ([`HELD_FILE`]).

use alloc::{boxed::Box, sync::Arc, vec::Vec};

use jvm::{ClassInstance, Jvm};
use spin::Mutex;

use wie_backend::System;
use wie_core_arm::{ArmCore, ResultWriter, SvcId};
use wie_util::{ByteRead, ByteWrite, Result};

use crate::runtime::SVC_CATEGORY_OZ;

const AID: &str = "0103CD8A";

/// The runtime's static call, `r0` the method.
const CALL: u32 = 0x1d72f2;
/// The runtime's virtual call, `r0` the method and `r1` the object.
const CALLV: u32 = 0x1d7300;
/// A static field's value, and setting it, `r0` the field and `r1` the value.
const GETSTATIC: u32 = 0x1d5f2c;
const PUTSTATIC: u32 = 0x1d64c8;
/// `network/b.a()B`.
const ONLINE: u32 = 0x1c587c;

/// Code changed, and what is there in the build this was written against.
struct Site {
    address: u32,
    code: [u16; 2],
}

/// `network/a.b(I[BI)V`, on an entry: `cmp r0,#2; beq party; ldr r0,=Z;
/// movs r1,#1`, then `Z = r1`. The `movs` becomes `adds r1,r4,#0`, the kind
/// read into r4.
const KEEP_KIND: Site = Site {
    address: 0x1bac62,
    code: [0x488a, 0x2101],
};
const KEEP_KIND_WITH: u16 = 0x1c21;

/// `network/b.a()B` once connected, with `Z` in r0: `negs r3,r0; orrs r3,r0;
/// lsrs r3,#31; adds r0,r3,#0; b out`, that is `Z != 0`. The first two become
/// a call to [`answer`], and the third the `b out`.
const ANSWER: Site = Site {
    address: 0x1c58ac,
    code: [0x4243, 0x4303],
};
const ANSWER_OUT: u16 = 0xe7f5;

/// `game/chars/m.e()V`, a monster falling, hands each thing it leaves to
/// `game/engine/f.a(SB)Z`, the number in r1. That counts it in a table of the
/// 140 things of the offline game, and a 코단 world monster leaves things
/// numbered past them - which only the server ever handed out - and the count
/// runs off the table's end. The call goes through [`leave`] instead.
const LEAVE: Site = Site {
    address: 0x197336,
    code: bl(0x197336, CALL),
};
/// The things the offline game has.
const THINGS: u16 = 140;

/// `game/chars/g.X()V`, every frame: the player with no health left is laid
/// down, `g.S()V`. The call goes through [`fall`].
const FALL: Site = Site {
    address: 0x153b10,
    code: bl(0x153b10, CALLV),
};
/// `network/b.^`, set to 10 by the server's word of the player falling
/// (`0x1c2e80`), and what opens 부활대기중 (`game/o`, `game/k`).
const WAITING: u32 = 0x1ff0fc;

/// The answer to 입구부활 (`0x1bf754`) stands the player at the entrance, on
/// this 구역 or after loading the first, and either way ends closing
/// 부활대기중: `network/b.^ = 0`. That setting goes through [`rise`].
///
/// Loading the first 구역 also puts the title to waiting on the server
/// (`network/f.a(B)V` with 2), which takes no keys. A party's server ended
/// that with the 구역's news once it was loaded; a dungeon the title fights
/// itself has none coming, so [`rise`] ends the wait as well.
const RISE: Site = Site {
    address: 0x1bf7e0,
    code: bl(0x1bf7e0, PUTSTATIC),
};
/// The methods [`rise`] calls, by the slots calls to them are made through:
/// `base/b.a()Lgame/chars/g;`, the player; `game/chars/e.i()S`, its health;
/// `e.d(I)V`, adding to the health up to the most; and [`STAND`].
const PLAYER: u32 = 0x1feb4c;
/// `network/f.a(B)V`, what the title waits on: 0 for nothing.
const WAIT: u32 = 0x1feb8c;
const HEALTH: u32 = 0x1ec8b8;
const HEAL: u32 = 0x1f09f4;
/// `game/chars/g.h()V`, the player back to standing. Loading a 구역 has it
/// stood again; one risen where it fell, on the 구역 it is on, is not.
const STAND: u32 = 0x1fbeb8;

/// The answer to a party room's start (`0x1bf53e`) loads the dungeon and puts
/// the title to waiting on the server, for the party's server to end with the
/// 구역's news once it is loaded. The call goes through [`start`] instead,
/// which has the dungeon fought as offline - as one entered from the map is,
/// whatever the area the rooms were opened from - and nothing waited for.
const START: Site = Site {
    address: 0x1bf560,
    code: bl(0x1bf560, CALL),
};
/// `network/b.Z`, the kind of the area the title is in.
const KIND: u32 = 0x1fe11c;

/// `game/data/k.a([BBBI)`, making an item the server sent - a ware, a thing
/// bought or one of the belongings - reads the item's own state
/// (`k.a([BIZ)I`) and then what the item is (`k.a([BIZZZ)I`): its price,
/// name, the level and classes it asks for, its might. That second part was
/// the server's copy of the title's own item tables, and the server is gone;
/// so the item is made from the tables instead, as the offline game makes
/// its shop's wares (`k.a(BBZZ)`, `0x115784`), and the second part is not
/// read. An item the tables do not know is made bare, as before.
const MADE: Site = Site {
    address: 0x114fb0,
    code: bl(0x114fb0, CALL),
};
const DEFINED: Site = Site {
    address: 0x114fe0,
    code: bl(0x114fe0, CALLV),
};
/// Its result is not used; the call becomes two `nop`s.
const NOTHING: [u16; 2] = [0x46c0, 0x46c0];
/// The slots of `k.a(BB)`, an item bare, and `k.a(BBZZ)`, one from the
/// tables.
const BARE: u32 = 0x1e7a48;
const FROM_THE_TABLES: u32 = 0x1e7ae0;

/// Code that is told it is offline in a dungeon: `[start, end)`, by the
/// address a call returns to.
const FIGHTING: [(u32, u32); 4] = [
    // game/chars/a to e.
    (0x117874, 0x136ab4),
    // game/engine/h, the skills.
    (0x13e664, 0x145c84),
    // game/chars/g.
    (0x14b260, 0x159774),
    // game/chars/h to n.
    (0x18dd58, 0x1a0600),
];

/// Installs the changes if this is the title and the code is the build they
/// were written against. `cave` is unmapped memory within a Thumb `bl` of the
/// image, for the code they call.
pub(crate) fn install_local_fights(core: &mut ArmCore, system: &System, jvm: &Jvm, cave: u32) -> Result<()> {
    if !system.aid().eq_ignore_ascii_case(AID) {
        return Ok(());
    }
    for site in [&KEEP_KIND, &ANSWER, &LEAVE, &FALL, &RISE, &START, &MADE, &DEFINED, &SENT] {
        if !halfwords_at(core, site.address, &site.code)? {
            tracing::warn!(
                "오즈: code at {:#x} differs from the known build; the 코단 world's fights stay the server's",
                site.address
            );
            return Ok(());
        }
    }

    let mut code = Vec::new();
    let mut place = |routine: &dyn Fn(u32) -> Vec<u8>| {
        let at = cave + code.len() as u32;
        code.extend_from_slice(&routine(at));
        at
    };
    let answer = place(&answer);
    let leave = place(&leave);
    let fall = place(&fall);
    let rise = place(&rise);
    let start = place(&start);
    let made = place(&made);
    core.register_svc_handler(
        SVC_CATEGORY_OZ,
        handle_oz_svc,
        &(system.clone(), jvm.clone(), Arc::new(Mutex::new(Vec::new()))),
    )?;
    let keep = core.make_svc_stub(SVC_CATEGORY_OZ, KEEP)?;
    let sent = place(&|at| sent(at, keep));
    core.load(&code, cave, code.len())?;

    write(core, KEEP_KIND.address + 2, &[KEEP_KIND_WITH])?;
    let [high, low] = bl(ANSWER.address, answer);
    write(core, ANSWER.address, &[high, low, ANSWER_OUT])?;
    write(core, LEAVE.address, &bl(LEAVE.address, leave))?;
    write(core, FALL.address, &bl(FALL.address, fall))?;
    write(core, RISE.address, &bl(RISE.address, rise))?;
    write(core, START.address, &bl(START.address, start))?;
    write(core, MADE.address, &bl(MADE.address, made))?;
    write(core, DEFINED.address, &NOTHING)?;
    write(core, SENT.address, &bl(SENT.address, sent))?;

    tracing::info!("오즈: the 코단 world's dungeons are fought by the title's offline rules");

    Ok(())
}

/// `network/b.a()B`'s answer, from `Z` in r0, with the caller's return address
/// at `[sp, #4]` where `b.a()B` pushed it.
fn answer(at: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0x0603); // lsls r3,r0,#24: Z, a byte
    let plain = code.branch(PL);
    code.op(0x9b01); // ldr r3,[sp,#4]
    code.op(0xb404); // push {r2}
    let mut offline = Vec::new();
    for (start, end) in FIGHTING {
        code.load(2, start);
        code.op(0x4293); // cmp r3,r2
        let before = code.branch(LO);
        code.load(2, end);
        code.op(0x4293);
        offline.push(code.branch(LO));
        code.land(before);
    }
    code.op(0xbc04); // pop {r2}
    code.op(0x2001); // movs r0,#1: a town's answer
    code.op(0x4770); // bx lr
    for branch in offline {
        code.land(branch);
    }
    code.op(0xbc04);
    code.op(0x2000); // movs r0,#0
    code.op(0x3801); // subs r0,#1: offline's
    code.op(0x4770);
    code.land(plain);
    code.op(0x4243); // negs r3,r0
    code.op(0x4303); // orrs r3,r0
    code.op(0x0fdb); // lsrs r3,r3,#31
    code.op(0x1c18); // adds r0,r3,#0
    code.op(0x4770);
    code.bytes()
}

/// `game/engine/f.a(SB)Z` for a thing the offline game has, false for one it
/// has not.
fn leave(at: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0x2900 | (THINGS - 1)); // cmp r1,#139
    let past = code.branch(HI);
    code.load(3, CALL | 1);
    code.op(0x4718); // bx r3, returning where the call would have
    code.land(past);
    code.op(0x2000); // movs r0,#0
    code.op(0x4770);
    code.bytes()
}

/// `g.S()V`, `r0` and `r1` as the call had them, after opening 부활대기중 as
/// the server's word did - where the title runs the area while connected, and
/// only once.
fn fall(at: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0xb503); // push {r0,r1,lr}
    code.call(ONLINE);
    code.op(0x0600); // lsls r0,r0,#24
    code.op(0x1600); // asrs r0,r0,#24
    code.op(0x2801); // cmp r0,#1
    let elsewhere = code.branch(NE);
    code.load(0, WAITING);
    code.call(GETSTATIC);
    code.op(0x2800); // cmp r0,#0
    let open = code.branch(NE);
    code.load(0, WAITING);
    code.op(0x210a); // movs r1,#10
    code.call(PUTSTATIC);
    code.land(elsewhere);
    code.land(open);
    code.op(0xbc03); // pop {r0,r1}
    code.op(0xbc08); // pop {r3}
    code.op(0x469e); // mov lr,r3
    code.load(3, CALLV | 1);
    code.op(0x4718); // bx r3
    code.bytes()
}

/// `network/b.^ = 0`, the risen player given its health back if it has none
/// and stood up, and the wait for the server ended.
fn rise(at: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0xb500); // push {lr}
    code.call(PUTSTATIC);
    code.load(0, PLAYER);
    code.call(CALL);
    code.op(0xb401); // push {r0}
    code.op(0x1c01); // adds r1,r0,#0
    code.load(0, HEALTH);
    code.call(CALLV);
    code.op(0x0400); // lsls r0,r0,#16
    code.op(0x9900); // ldr r1,[sp]
    code.op(0x2800); // cmp r0,#0
    let alive = code.branch(GT);
    code.load(0, HEAL);
    code.load(2, 30000);
    code.call(CALLV);
    code.land(alive);
    code.op(0xbc02); // pop {r1}
    code.load(0, STAND);
    code.call(CALLV);
    code.load(0, WAIT);
    code.op(0x2100); // movs r1,#0
    code.call(CALL);
    code.op(0xbd00); // pop {pc}
    code.bytes()
}

/// `network/b.Z = -1`, and `network/f.a(B)V` with 0 for the 2 it was called
/// with.
fn start(at: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0xb500); // push {lr}
    code.load(0, KIND);
    code.op(0x2100); // movs r1,#0
    code.op(0x3901); // subs r1,#1
    code.call(PUTSTATIC);
    code.load(0, WAIT);
    code.op(0x2100); // movs r1,#0
    code.call(CALL);
    code.op(0xbd00); // pop {pc}
    code.bytes()
}

/// `k.a(BBZZ)` for the kind in r1 and the model in r2, as the offline shop
/// asks it (`0x115774`), or `k.a(BB)` when the tables have no such item.
fn made(at: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0xb506); // push {r1,r2,lr}
    code.op(0x2300); // movs r3,#0
    code.op(0xb408); // push {r3}: the last argument
    code.op(0x2301); // movs r3,#1
    code.load(0, FROM_THE_TABLES);
    code.call(CALL);
    code.op(0xb001); // add sp,#4
    code.op(0x2800); // cmp r0,#0
    let found = code.branch(NE);
    code.op(0x9900); // ldr r1,[sp]
    code.op(0x9a01); // ldr r2,[sp,#4]
    code.load(0, BARE);
    code.call(CALL);
    code.land(found);
    code.op(0xb002); // add sp,#8
    code.op(0xbd00); // pop {pc}
    code.bytes()
}

/// `network/a.a(II)V`, sending a request, has it built by `network/a.a(II)[B`
/// with the kind in r1 and the command in r2. The call goes through [`sent`],
/// which first has [`KEEP`] look at what the player holds.
const SENT: Site = Site {
    address: 0x1b5412,
    code: bl(0x1b5412, CALL),
};
const KEEP: u32 = 1;

/// [`KEEP`] with the kind and the command, then the call it stands for.
fn sent(at: u32, keep: u32) -> Vec<u8> {
    let mut code = Thumb::new(at);
    code.op(0xb507); // push {r0,r1,r2,lr}
    code.op(0x1c08); // adds r0,r1,#0
    code.op(0x1c11); // adds r1,r2,#0
    code.load(3, keep);
    code.op(0x4798); // blx r3
    code.op(0xbc07); // pop {r0,r1,r2}
    code.op(0xbc08); // pop {r3}
    code.op(0x469e); // mov lr,r3
    code.load(3, CALL | 1);
    code.op(0x4718); // bx r3
    code.bytes()
}

/// The file [`KEEP`] writes, which the emulated 코단 world server
/// (`wie_backend`'s `kodan`) takes on the request that follows:
/// `[u64 gold][u16 count]` and that many `[kind][model][count][slot][bag]
/// [u64 id]`.
const HELD_FILE: &str = "kodan_held";

/// `network/b.a`, the belongings: a `game/g`, a `Vector` of `game/data/k`.
const BELONGINGS: (&str, &str, &str) = ("network/b", "a", "Lgame/g;");
/// `network/b.a`, the counters the server tells (`0x1bb70c`), the gold
/// second.
const COUNTERS: (&str, &str, &str) = ("network/b", "a", "[J");
const GOLD: usize = 1;
/// The fields of a `game/data/k` [`HELD_FILE`] keeps, in its order: the
/// kind, the model, the count, the slot it is worn in and the bag.
const ITEM_BYTES: [&str; 5] = ["l", "m", "n", "o", "p"];
/// And its id.
const ITEM_ID: &str = "b";

async fn handle_oz_svc(core: &mut ArmCore, (system, jvm, kept): &mut (System, Jvm, Arc<Mutex<Vec<u8>>>), id: SvcId) -> Result<()> {
    let (_, lr) = core.read_pc_lr()?;

    if id.0 == KEEP {
        let framed = jvm.try_push_native_frame(Vec::new());
        let held = held(jvm).await;
        if framed {
            jvm.pop_frame();
        }
        if let Some(held) = held
            && *kept.lock() != held
        {
            let files = system.filesystem();
            files.truncate(HELD_FILE, 0).await;
            files.write(HELD_FILE, 0, &held).await;
            *kept.lock() = held;
        }
    }

    ().write(core, lr)
}

/// What the player holds and its gold, as [`HELD_FILE`] has them; `None`
/// before the title has belongings to read.
async fn held(jvm: &Jvm) -> Option<Vec<u8>> {
    let (class, name, descriptor) = BELONGINGS;
    let belongings: Box<dyn ClassInstance> = jvm.get_static_field::<Option<_>>(class, name, descriptor).await.ok()??;
    let (class, name, descriptor) = COUNTERS;
    let counters: Box<dyn ClassInstance> = jvm.get_static_field::<Option<_>>(class, name, descriptor).await.ok()??;
    let gold: i64 = *jvm.load_array::<i64>(&counters, GOLD, 1).await.ok()?.first()?;

    let count: i32 = jvm.invoke_virtual(&belongings, "size", "()I", ()).await.ok()?;
    let mut held = (gold as u64).to_be_bytes().to_vec();
    held.extend_from_slice(&(count as u16).to_be_bytes());
    for index in 0..count {
        let item: Box<dyn ClassInstance> = jvm
            .invoke_virtual::<_, Option<_>>(&belongings, "elementAt", "(I)Ljava/lang/Object;", (index,))
            .await
            .ok()??;
        for field in ITEM_BYTES {
            held.push(jvm.get_field::<i8>(&item, field, "B").await.ok()? as u8);
        }
        held.extend_from_slice(&jvm.get_field::<i64>(&item, ITEM_ID, "J").await.ok()?.to_be_bytes());
    }
    Some(held)
}

/// Thumb condition codes.
const NE: u16 = 0x1;
const LO: u16 = 0x3;
const PL: u16 = 0x5;
const HI: u16 = 0x8;
const GT: u16 = 0xc;

/// Thumb code built a halfword at a time, the words it loads placed after it.
/// Its place, and so its length, are 4-aligned.
struct Thumb {
    at: u32,
    code: Vec<u16>,
    /// Each `ldr rX,[pc,#n]`: where it is, and the word it loads.
    loads: Vec<(usize, u32)>,
}

impl Thumb {
    fn new(at: u32) -> Self {
        debug_assert_eq!(at % 4, 0);
        Self {
            at,
            code: Vec::new(),
            loads: Vec::new(),
        }
    }

    fn op(&mut self, op: u16) {
        self.code.push(op);
    }

    /// `bl to`.
    fn call(&mut self, to: u32) {
        let here = self.at + 2 * self.code.len() as u32;
        self.code.extend_from_slice(&bl(here, to));
    }

    /// `ldr reg,=word`.
    fn load(&mut self, reg: u16, word: u32) {
        self.loads.push((self.code.len(), word));
        self.code.push(0x4800 | (reg << 8));
    }

    /// A conditional branch forward, to where [`Thumb::land`] is called.
    fn branch(&mut self, condition: u16) -> usize {
        self.code.push(0xd000 | (condition << 8));
        self.code.len() - 1
    }

    fn land(&mut self, branch: usize) {
        let offset = self.code.len() - branch - 2;
        debug_assert!(offset < 0x80);
        self.code[branch] |= offset as u16;
    }

    fn bytes(mut self) -> Vec<u8> {
        if self.code.len() % 2 == 1 {
            self.code.push(0x46c0); // mov r8,r8
        }
        let pool = self.at + 2 * self.code.len() as u32;
        for (index, &(at, _)) in self.loads.iter().enumerate() {
            let pc = (self.at + 2 * at as u32 + 4) & !3;
            let offset = (pool + 4 * index as u32 - pc) / 4;
            debug_assert!(offset < 0x100);
            self.code[at] |= offset as u16;
        }

        let mut bytes: Vec<u8> = self.code.iter().flat_map(|x| x.to_le_bytes()).collect();
        for (_, word) in &self.loads {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}

const fn bl(from: u32, to: u32) -> [u16; 2] {
    let offset = to.wrapping_sub(from + 4) as i32;

    [0xf000 | ((offset >> 12) as u16 & 0x7ff), 0xf800 | ((offset >> 1) as u16 & 0x7ff)]
}

fn write(core: &mut ArmCore, address: u32, code: &[u16]) -> Result<()> {
    let bytes: Vec<u8> = code.iter().flat_map(|x| x.to_le_bytes()).collect();
    core.write_bytes(address, &bytes)
}

fn halfwords_at(core: &ArmCore, address: u32, expected: &[u16]) -> Result<bool> {
    let mut bytes = alloc::vec![0; expected.len() * 2];
    core.read_bytes(address, &mut bytes)?;

    Ok(bytes.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).eq(expected.iter().copied()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hooked_calls_are_the_ones_read_out_of_the_build() {
        assert_eq!(LEAVE.code, [0xf03f, 0xffdc]);
        assert_eq!(FALL.code, [0xf083, 0xfbf6]);
        assert_eq!(RISE.code, [0xf016, 0xfe72]);
        assert_eq!(START.code, [0xf017, 0xfec7]);
        assert_eq!(MADE.code, [0xf0c2, 0xf99f]);
        assert_eq!(DEFINED.code, [0xf0c2, 0xf98e]);
        assert_eq!(SENT.code, [0xf021, 0xff6e]);
    }

    #[test]
    fn a_load_reaches_its_word_after_the_code() {
        let leave = leave(0x1000);
        let halfword = |at: usize| u16::from_le_bytes([leave[at], leave[at + 1]]);

        assert_eq!(leave.len(), 16);
        // `bhi past` lands past the load and the `bx`.
        assert_eq!(halfword(2), 0xd801);
        // `ldr r3,[pc,#4]` at +4 loads the word at +12.
        assert_eq!(halfword(4), 0x4b01);
        assert_eq!(u32::from_le_bytes(leave[12..16].try_into().unwrap()), CALL | 1);
    }
}
