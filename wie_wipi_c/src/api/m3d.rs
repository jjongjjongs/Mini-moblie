//! `m3dInterf` - the KTF 3D library (`0102B95C`), Mascot Capsule V3 as a
//! WIPI C interface.
//!
//! 싸이 디럭스 (KTF `0103629C`) draws its dice and its logo with it: its
//! archive carries Mascot Capsule figures (`.mbac`), actions (`.mtra`) and
//! textures (`.bmp`). At start it asks `MC_knlGetExecNames("0102B95C")`,
//! `MC_knlLoad`s it and takes the table from `MC_knlGetDLLInterface`, then
//! copies all of its slots into globals of its own (`0x11823c`). With no
//! table, that copy read address 0 and the run ended.
//!
//! The library is C++: the title allocates each object itself and the table's
//! functions are its constructors and methods, `this` in `r0`. Objects are
//! kept here by that address. Data comes in through a stream object of the
//! title's (`0x101208` builds one) whose bytes are at `+4` and length at `+8`.
//!
//! The slots 싸이 디럭스 calls, read off what it hands each one and what it
//! does with the answer. They follow the Java API (`com.mascotcapsule.
//! micro3d.v3`) closely, except that the layout and the effect live on the
//! `Graphics3D` itself:
//!
//! | slot | what | as the title calls it |
//! |------|------|-----------------------|
//! | 0 | `ActionTable::ActionTable()` | on each of its two tables |
//! | 2 | `ActionTable::load(stream)` | `.mtra`; true when it took |
//! | 4 | `ActionTable::getNumFrames(action)` | in 1/65536 of a keyframe; the logo waits on it |
//! | 6 | `AffineTrans::set(m00 .. m23)` | twelve words after `this` |
//! | 9, 10 | `AffineTrans::mul(a, b)` | `this = a * b` |
//! | 11, 12, 13 | `AffineTrans::rotationX/Y/Z(angle)` | 4096 to a turn; the translation stays |
//! | 15 | `AffineTrans::lookAt(position, look, up)` | the camera |
//! | 40 | `Figure::Figure()` | on each of its eleven figures |
//! | 42 | `Figure::load(stream)` | `.mbac`; true when it took |
//! | 43 | `Figure::setPosture(table, action, frame)` | each frame of the logo |
//! | 48 | `Graphics3D::Graphics3D()` | once |
//! | 51 | `bind(width, height, pitch, pixels)` | the 16-bit screen the title flushes itself |
//! | 52 | `setClip(x, y, width, height)` | |
//! | 53 | `setAffineTrans(affine)` | the camera, as `FigureLayout` has it |
//! | 54 | `setCenter(x, y)` | |
//! | 55 | `setPerspective(near, far, angle)` | |
//! | 59, 60 | set and get the attributes | bit 0 is lighting: on for the logo, off for the board |
//! | 65 | `setTexture(texture)` | what figures and lists draw with |
//! | 67 | `flush()` | the depth buffer starts over |
//! | 68 | `renderFigure(figure)` | the logo, the character, the dice |
//! | 74 | `drawCommandList({length, words})` | the board's tiles |
//! | 86 | `Texture::Texture()` | on each of its three textures |
//! | 88 | `Texture::load(stream, for_model)` | `.bmp`; true when it took |
//!
//! Slot 23 is handed an action table and a figure once each at load; what it
//! does is not known, and it answers zero as every other slot does.
//!
//! Drawing is [`render`]'s: back faces are the ones that wind counter-clockwise on
//! screen, and with them culled the logo shows its lettered faces the right
//! way round - drawn the other way, it showed the insides of the cube.

mod loader;
mod render;

use alloc::{boxed::Box, collections::BTreeMap, format, vec, vec::Vec};

use wipi_types::wipic::WIPICWord;

use wie_util::{Result, read_generic, write_generic};

use crate::context::WIPICContext;

/// The name a title asks `MC_knlGetDLLInterface` for.
pub const INTERFACE_NAME: &str = "m3dInterf";

/// The library's own program id.
pub const LIBRARY_ID: &str = "0102B95C";

/// How many functions the table holds: 싸이 디럭스 copies slots 0 to 97.
pub const SLOTS: u16 = 98;

enum Object {
    ActionTable(Vec<loader::Action>),
    Figure(Figure),
    Texture(Option<Texture>),
    Graphics3D(Box<Graphics>),
}

#[derive(Default)]
struct Figure {
    model: Option<loader::Figure>,
    /// The action table, action and frame (in 1/65536 keyframes) it is posed
    /// in, as `setPosture` left it.
    posture: Option<(WIPICWord, usize, i32)>,
    patterns: u32,
}

/// What a `Graphics3D` draws with and onto.
struct Graphics {
    /// The 16-bit pixels it draws onto: address, width, height, pitch in
    /// pixels.
    target: Option<(WIPICWord, u32, u32, u32)>,
    clip: (i32, i32, i32, i32),
    affine: [i32; 12],
    center: (i32, i32),
    /// Near, far and the view angle, in 1/4096 of a turn.
    perspective: (i32, i32, i32),
    texture: Option<WIPICWord>,
    flags: WIPICWord,
    depth: Vec<f32>,
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            target: None,
            clip: (0, 0, 0, 0),
            affine: [4096, 0, 0, 0, 0, 4096, 0, 0, 0, 0, 4096, 0],
            center: (0, 0),
            perspective: (1, 32767, 512),
            texture: None,
            flags: 0,
            depth: Vec::new(),
        }
    }
}

struct Texture {
    width: u32,
    height: u32,
    /// Palette index per pixel, top row first.
    pixels: Vec<u8>,
    palette: Vec<[u8; 3]>,
}

/// Every object a title has made, by its address.
#[derive(Default)]
pub struct State {
    objects: BTreeMap<WIPICWord, Object>,
}

/// The bytes behind one of the title's stream objects.
fn read_stream(context: &dyn WIPICContext, stream: WIPICWord) -> Result<Vec<u8>> {
    let data: WIPICWord = read_generic(context, stream + 4)?;
    let size: WIPICWord = read_generic(context, stream + 8)?;
    let mut bytes = vec![0; size as usize];
    context.read_bytes(data, &mut bytes)?;

    Ok(bytes)
}

/// A Mascot Capsule texture: an uncompressed 8-bit Windows bitmap.
fn load_texture(data: &[u8]) -> Option<Texture> {
    let word = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?)) };
    let half = |at: usize| -> Option<u16> { Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?)) };
    if data.get(0..2)? != b"BM" {
        return None;
    }
    let offset = word(10)? as usize;
    let header = word(14)? as usize;
    let width = word(18)? as i32;
    let height = word(22)? as i32;
    if half(28)? != 8 || word(30)? != 0 || width <= 0 || height == 0 {
        return None;
    }
    let colors = match word(46)? {
        0 => 256,
        n => n as usize,
    };
    let palette = (0..colors)
        .map(|i| {
            let at = 14 + header + i * 4;
            Some([*data.get(at + 2)?, *data.get(at + 1)?, *data.get(at)?])
        })
        .collect::<Option<Vec<_>>>()?;

    let (width, rows) = (width as u32, height.unsigned_abs());
    let stride = (width as usize).div_ceil(4) * 4;
    let mut pixels = vec![0; (width * rows) as usize];
    for row in 0..rows as usize {
        // Bottom-up unless the height says otherwise.
        let source = if height > 0 { rows as usize - 1 - row } else { row };
        let line = data.get(offset + source * stride..offset + source * stride + width as usize)?;
        pixels[row * width as usize..(row + 1) * width as usize].copy_from_slice(line);
    }

    Some(Texture {
        width,
        height: rows,
        pixels,
        palette,
    })
}

/// An `AffineTrans`: twelve words, `m00` to `m23`.
fn read_affine(context: &dyn WIPICContext, address: WIPICWord) -> Result<[i32; 12]> {
    let mut m = [0i32; 12];
    for (i, value) in m.iter_mut().enumerate() {
        *value = read_generic(context, address + i as u32 * 4)?;
    }
    Ok(m)
}

fn write_affine(context: &mut dyn WIPICContext, address: WIPICWord, m: &[i32; 12]) -> Result<()> {
    for (i, value) in m.iter().enumerate() {
        write_generic(context, address + i as u32 * 4, *value)?;
    }
    Ok(())
}

/// `a * b` the way `AffineTrans::mul` rounds it: rotations in 1/4096.
fn mul_affine(a: &[i32; 12], b: &[i32; 12]) -> [i32; 12] {
    let mut m = [0i32; 12];
    for row in 0..3 {
        let l = &a[row * 4..row * 4 + 4];
        for col in 0..4 {
            let sum = l[0] as i64 * b[col] as i64 + l[1] as i64 * b[4 + col] as i64 + l[2] as i64 * b[8 + col] as i64;
            let mut value = ((sum + 2048) >> 12) as i32;
            if col == 3 {
                value += l[3];
            }
            m[row * 4 + col] = value;
        }
    }
    m
}

/// `Util3D::sin`, a turn being 4096.
fn sin(angle: i32) -> i32 {
    libm::round(libm::sin(angle as f64 * core::f64::consts::PI / 2048.0) * 4096.0) as i32
}

/// `Util3D::cos`, a turn being 4096.
fn cos(angle: i32) -> i32 {
    libm::round(libm::cos(angle as f64 * core::f64::consts::PI / 2048.0) * 4096.0) as i32
}

fn read_vector(context: &dyn WIPICContext, address: WIPICWord) -> Result<[f64; 3]> {
    Ok([
        read_generic::<i32, _>(context, address)? as f64,
        read_generic::<i32, _>(context, address + 4)? as f64,
        read_generic::<i32, _>(context, address + 8)? as f64,
    ])
}

/// `AffineTrans::lookAt(position, look, up)`: the camera at `position`,
/// looking along `look`, rows in 1/4096.
fn look_at(context: &mut dyn WIPICContext, this: WIPICWord, position: WIPICWord, look: WIPICWord, up: WIPICWord) -> Result<()> {
    let (pos, look, up) = (read_vector(context, position)?, read_vector(context, look)?, read_vector(context, up)?);
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let unit = |v: [f64; 3]| {
        let length = libm::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
        if length == 0.0 {
            [0.0; 3]
        } else {
            [v[0] * 4096.0 / length, v[1] * 4096.0 / length, v[2] * 4096.0 / length]
        }
    };
    let x = unit(cross(look, up));
    let y = unit(cross(look, x));
    let z = unit(look);
    let mut m = [0i32; 12];
    for (row, axis) in [x, y, z].iter().enumerate() {
        let axis = [libm::round(axis[0]), libm::round(axis[1]), libm::round(axis[2])];
        m[row * 4] = axis[0] as i32;
        m[row * 4 + 1] = axis[1] as i32;
        m[row * 4 + 2] = axis[2] as i32;
        m[row * 4 + 3] = libm::round(-(pos[0] * axis[0] + pos[1] * axis[1] + pos[2] * axis[2]) / 4096.0) as i32;
    }
    write_affine(context, this, &m)
}

/// What a draw call hands over.
enum Source {
    /// `Graphics3D::renderFigure(figure)`: the figure, posed as it was last set.
    Figure(WIPICWord),
    /// `Graphics3D::drawCommandList(commands)`: a Mascot Capsule command list.
    Commands(Vec<i32>),
}

const COMMAND_LIST_VERSION: i32 = 0xfe000001u32 as i32;

/// Walks a command list: its primitives as triangles, with what the list
/// itself changes about the view.
fn command_triangles(commands: &[i32], view: &mut render::View) -> Vec<render::Triangle> {
    const END: u32 = 0x80;
    const NOP: u32 = 0x81;
    const CLIP: u32 = 0x84;
    const CENTER: u32 = 0x85;
    const PARALLEL_SCALE: u32 = 0x90;
    const PARALLEL_SIZE: u32 = 0x91;
    const PERSPECTIVE_FOV: u32 = 0x92;
    const PERSPECTIVE_WH: u32 = 0x93;
    const AMBIENT_LIGHT: u32 = 0xa0;
    const DIRECTION_LIGHT: u32 = 0xa1;
    const THRESHOLD: u32 = 0xaf;

    let mut out = Vec::new();
    if commands.first() != Some(&COMMAND_LIST_VERSION) {
        return out;
    }
    let mut i = 1;
    let next = |i: &mut usize| {
        let value = commands.get(*i).copied().unwrap_or(0);
        *i += 1;
        value
    };
    while i < commands.len() {
        let command = next(&mut i) as u32;
        match command >> 24 {
            END => break,
            NOP => i += (command & 0xff_ffff) as usize,
            CLIP => {
                let (x, y, w, h) = (next(&mut i), next(&mut i), next(&mut i), next(&mut i));
                view.clip = (x.max(view.clip.0), y.max(view.clip.1), w.min(view.clip.2), h.min(view.clip.3));
            }
            CENTER => view.center = (next(&mut i), next(&mut i)),
            PARALLEL_SCALE | PARALLEL_SIZE => i += 2,
            PERSPECTIVE_FOV => {
                let (near, far, angle) = (next(&mut i), next(&mut i), next(&mut i));
                view.near = near as f32;
                view.far = far as f32;
                view.focal = focal(view.clip.2, angle);
            }
            PERSPECTIVE_WH => i += 4,
            AMBIENT_LIGHT | THRESHOLD => i += if command >> 24 == AMBIENT_LIGHT { 1 } else { 3 },
            DIRECTION_LIGHT => i += 4,
            _ => {
                let kind = (command >> 24) & 7;
                let corners = match kind {
                    3 => 3,
                    4 => 4,
                    1 | 2 | 5 => {
                        // Points, lines and sprites are not drawn; their data is
                        // still stepped over.
                        let count = ((command >> 16) & 0xff) as usize;
                        let per = if kind == 2 { 2 } else { 1 };
                        i += count * 3 * per;
                        if command & 0xc00 == 0x400 {
                            i += 1;
                        } else if command & 0xc00 == 0x800 {
                            i += count;
                        }
                        if command & 0x3000 != 0 {
                            i += if kind == 5 {
                                if command & 0x3000 == 0x1000 { 8 } else { count * 8 }
                            } else {
                                count * 2 * per
                            };
                        }
                        continue;
                    }
                    _ => continue,
                };
                let count = ((command >> 16) & 0xff) as usize;
                let vertices: Vec<[f32; 3]> = (0..count * corners)
                    .map(|_| [next(&mut i) as f32, next(&mut i) as f32, next(&mut i) as f32])
                    .collect();
                let read_normal = |i: &mut usize| [next(i) as f32, next(i) as f32, next(i) as f32];
                let normals: Vec<[f32; 3]> = match command & 0x300 {
                    0x200 => (0..count)
                        .flat_map(|_| [read_normal(&mut i); 1])
                        .flat_map(|n| core::iter::repeat_n(n, corners))
                        .collect(),
                    0x300 => (0..count * corners).map(|_| read_normal(&mut i)).collect(),
                    _ => Vec::new(),
                };
                let colors: Vec<i32> = match command & 0xc00 {
                    0x400 => vec![next(&mut i); count],
                    0x800 => (0..count).map(|_| next(&mut i)).collect(),
                    _ => Vec::new(),
                };
                let uvs: Vec<[f32; 2]> = if command & 0x3000 != 0 {
                    (0..count * corners).map(|_| [next(&mut i) as f32, next(&mut i) as f32]).collect()
                } else {
                    Vec::new()
                };
                for n in 0..count {
                    let base = n * corners;
                    let color = colors.get(n).map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).unwrap_or([255; 3]);
                    let fans: &[[usize; 3]] = if corners == 4 { &[[0, 1, 2], [3, 0, 2]] } else { &[[0, 1, 2]] };
                    for fan in fans {
                        out.push(render::Triangle {
                            points: fan.map(|k| vertices[base + k]),
                            uv: (!uvs.is_empty()).then(|| fan.map(|k| uvs[base + k])),
                            color,
                            color_key: command & 0x10 != 0,
                            double_sided: true,
                            blend: (command & 0x60) >> 4,
                            normals: (command & 1 != 0 && !normals.is_empty()).then(|| fan.map(|k| normals[base + k])),
                        });
                    }
                }
            }
        }
    }
    out
}

/// The focal length in pixels for a view `width` wide at `angle` (1/4096 of a
/// turn across the whole view).
fn focal(width: i32, angle: i32) -> f32 {
    (width.max(1) as f32 / 2.0) / libm::tanf(angle as f32 / 4096.0 * core::f32::consts::PI)
}

/// Draws onto the pixels the `Graphics3D` at `this` is bound to.
///
/// Only the rectangle the geometry covers is read out of the title's memory
/// and written back: the board is thirty-odd command lists a frame, and a
/// whole screen each time is most of the cost of drawing it.
fn render_into(context: &mut dyn WIPICContext, this: WIPICWord, source: Source) -> Result<()> {
    let state = context.kernel_state();
    let mut guard = state.lock();
    let objects = &mut guard.m3d.objects;
    let Some(Object::Graphics3D(mut graphics)) = objects.remove(&this) else {
        return Ok(());
    };
    let Some((address, width, height, pitch)) = graphics.target else {
        objects.insert(this, Object::Graphics3D(graphics));
        return Ok(());
    };

    let (near, far, angle) = graphics.perspective;
    let mut view = render::View {
        affine: graphics.affine,
        center: graphics.center,
        near: near as f32,
        far: far as f32,
        focal: focal(graphics.clip.2, angle),
        clip: graphics.clip,
        // Mascot Capsule's default light: straight along the view, at full
        // strength, with no ambient - on while the attribute's lighting bit is.
        light: (graphics.flags & 1 != 0).then_some(([0.0, 0.0, 1.0], 1.0, 0.0)),
    };
    let triangles = match source {
        Source::Figure(figure) => match objects.get(&figure) {
            Some(Object::Figure(figure)) => match &figure.model {
                Some(model) => {
                    let action = match figure.posture {
                        Some((table, action, frame)) => match objects.get(&table) {
                            Some(Object::ActionTable(actions)) => actions.get(action).map(|a| (a, frame)),
                            _ => None,
                        },
                        None => None,
                    };
                    render::figure_triangles(model, &render::pose(model, action), figure.patterns)
                }
                None => Vec::new(),
            },
            _ => Vec::new(),
        },
        Source::Commands(commands) => command_triangles(&commands, &mut view),
    };
    let (projected, (left, top, right, bottom)) = render::project(&view, &triangles, width, height);

    if !projected.is_empty() {
        let texture = match graphics.texture.and_then(|t| objects.get(&t)) {
            Some(Object::Texture(Some(texture))) => Some(render::Texels {
                width: texture.width,
                height: texture.height,
                pixels: &texture.pixels,
                palette: &texture.palette,
            }),
            _ => None,
        };
        if graphics.depth.len() != (width * height) as usize {
            graphics.depth = render::clear_depth(width, height);
        }

        let columns = right - left;
        let mut row = vec![0u8; columns as usize * 2];
        let mut pixels = vec![0u16; (columns * (bottom - top)) as usize];
        for y in top..bottom {
            context.read_bytes(address + (y as u32 * pitch + left as u32) * 2, &mut row)?;
            for (x, pixel) in row.chunks_exact(2).enumerate() {
                pixels[((y - top) * columns) as usize + x] = u16::from_le_bytes([pixel[0], pixel[1]]);
            }
        }

        let mut target = render::Target {
            width,
            left,
            top,
            columns,
            pixels: &mut pixels,
            depth: &mut graphics.depth,
        };
        render::draw(&mut target, &view, &projected, texture.as_ref());

        for y in top..bottom {
            for (x, pixel) in row.chunks_exact_mut(2).enumerate() {
                pixel.copy_from_slice(&pixels[((y - top) * columns) as usize + x].to_le_bytes());
            }
            context.write_bytes(address + (y as u32 * pitch + left as u32) * 2, &row)?;
        }
    }

    drop(projected);
    drop(triangles);
    objects.insert(this, Object::Graphics3D(graphics));

    Ok(())
}

fn describe(args: &[WIPICWord]) -> alloc::string::String {
    args.iter().map(|a| format!("{a:#x}")).collect::<Vec<_>>().join(", ")
}

/// One call into the table, by slot.
pub async fn call(context: &mut dyn WIPICContext, slot: u16, args: [WIPICWord; 13]) -> Result<WIPICWord> {
    let this = args[0];
    let state = context.kernel_state();

    let answer = match slot {
        0 => {
            state.lock().m3d.objects.insert(this, Object::ActionTable(Vec::new()));
            0
        }
        2 => {
            let bytes = read_stream(context, args[1])?;
            let actions = loader::load_actions(&bytes);
            tracing::debug!(
                "m3d ActionTable::load({this:#x}, {} bytes) -> {:?}",
                bytes.len(),
                actions.as_ref().map(|a| a.len())
            );
            let ok = actions.is_ok();
            state.lock().m3d.objects.insert(this, Object::ActionTable(actions.unwrap_or_default()));
            ok as WIPICWord
        }
        4 => match state.lock().m3d.objects.get(&this) {
            Some(Object::ActionTable(actions)) => actions.get(args[1] as usize).map(|a| a.keyframes << 16).unwrap_or(0),
            _ => 0,
        },
        40 => {
            state.lock().m3d.objects.insert(this, Object::Figure(Figure::default()));
            0
        }
        42 => {
            let bytes = read_stream(context, args[1])?;
            let figure = loader::load_figure(&bytes);
            tracing::debug!(
                "m3d Figure::load({this:#x}, {} bytes) -> {:?}",
                bytes.len(),
                figure.as_ref().map(|f| (f.vertices.len(), f.polygons.len()))
            );
            let ok = figure.is_ok();
            state.lock().m3d.objects.insert(
                this,
                Object::Figure(Figure {
                    model: figure.ok(),
                    ..Default::default()
                }),
            );
            ok as WIPICWord
        }
        48 => {
            state.lock().m3d.objects.insert(this, Object::Graphics3D(Box::default()));
            0
        }
        6 => {
            write_affine(context, this, &core::array::from_fn(|i| args[i + 1] as i32))?;
            0
        }
        9 | 10 => {
            let product = mul_affine(&read_affine(context, args[1])?, &read_affine(context, args[2])?);
            write_affine(context, this, &product)?;
            0
        }
        11..=13 => {
            let mut m = read_affine(context, this)?;
            let (s, c) = (sin(args[1] as i32), cos(args[1] as i32));
            let rotation = match slot {
                11 => [4096, 0, 0, 0, c, -s, 0, s, c],
                12 => [c, 0, s, 0, 4096, 0, -s, 0, c],
                _ => [c, -s, 0, s, c, 0, 0, 0, 4096],
            };
            for row in 0..3 {
                m[row * 4..row * 4 + 3].copy_from_slice(&rotation[row * 3..row * 3 + 3]);
            }
            write_affine(context, this, &m)?;
            0
        }
        15 => {
            look_at(context, this, args[1], args[2], args[3])?;
            0
        }
        43 => {
            if let Some(Object::Figure(figure)) = state.lock().m3d.objects.get_mut(&this) {
                figure.posture = Some((args[1], args[2] as usize, args[3] as i32));
            }
            0
        }
        51 | 52 | 53 | 54 | 55 | 59 | 65 => {
            let affine = if slot == 53 { Some(read_affine(context, args[1])?) } else { None };
            let mut state = state.lock();
            let Some(Object::Graphics3D(graphics)) = state.m3d.objects.get_mut(&this) else {
                return Ok(0);
            };
            match slot {
                51 => {
                    graphics.target = Some((args[4], args[1], args[2], args[3]));
                    graphics.clip = (0, 0, args[1] as i32, args[2] as i32);
                    graphics.depth = render::clear_depth(args[1], args[2]);
                }
                52 => graphics.clip = (args[1] as i32, args[2] as i32, args[3] as i32, args[4] as i32),
                53 => graphics.affine = affine.unwrap_or(graphics.affine),
                54 => graphics.center = (args[1] as i32, args[2] as i32),
                55 => graphics.perspective = (args[1] as i32, args[2] as i32, args[3] as i32),
                59 => graphics.flags = args[1],
                _ => graphics.texture = Some(args[1]),
            }
            0
        }
        60 => match state.lock().m3d.objects.get(&this) {
            Some(Object::Graphics3D(graphics)) => graphics.flags,
            _ => 0,
        },
        67 => {
            if let Some(Object::Graphics3D(graphics)) = state.lock().m3d.objects.get_mut(&this)
                && let Some((_, width, height, _)) = graphics.target
            {
                graphics.depth = render::clear_depth(width, height);
            }
            0
        }
        68 => {
            render_into(context, this, Source::Figure(args[1]))?;
            0
        }
        86 => {
            state.lock().m3d.objects.insert(this, Object::Texture(None));
            0
        }
        88 => {
            let bytes = read_stream(context, args[1])?;
            let texture = load_texture(&bytes);
            tracing::debug!(
                "m3d Texture::load({this:#x}, {} bytes, {}) -> {:?}",
                bytes.len(),
                args[2],
                texture.as_ref().map(|t| (t.width, t.height))
            );
            let ok = texture.is_some();
            state.lock().m3d.objects.insert(this, Object::Texture(texture));
            ok as WIPICWord
        }
        74 => {
            // The list arrives as an array of the title's: its length, then
            // where its words are.
            let length: WIPICWord = read_generic(context, args[1])?;
            let words: WIPICWord = read_generic(context, args[1] + 4)?;
            let commands = (0..length.min(0x10000))
                .map(|i| read_generic::<i32, _>(context, words + i * 4))
                .collect::<Result<Vec<_>>>()?;
            render_into(context, this, Source::Commands(commands))?;
            0
        }
        _ => {
            tracing::warn!("m3d-{slot}({})", describe(&args));
            0
        }
    };

    Ok(answer)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec::Vec;

    use wie_util::{read_generic, write_generic};

    use crate::context::{WIPICContext, test::TestContext};

    use super::{call, command_triangles, load_texture, loader, render};

    fn words(context: &mut TestContext, address: u32, values: &[i32]) {
        for (i, value) in values.iter().enumerate() {
            write_generic(context, address + i as u32 * 4, *value).unwrap();
        }
    }

    fn affine(context: &TestContext, address: u32) -> [i32; 12] {
        core::array::from_fn(|i| read_generic(context, address + i as u32 * 4).unwrap())
    }

    fn args(values: &[u32]) -> [u32; 13] {
        core::array::from_fn(|i| values.get(i).copied().unwrap_or(0))
    }

    #[futures_test::test]
    async fn an_affine_is_set_rotated_and_multiplied_as_mascot_rounds_it() {
        let mut context = TestContext::new();
        let (a, b, product) = (0x1000, 0x1100, 0x1200);

        // set(4096, 0, 0, 10, 0, 4096, 0, 20, 0, 0, 4096, 30)
        call(&mut context, 6, args(&[a, 4096, 0, 0, 10, 0, 4096, 0, 20, 0, 0, 4096, 30]))
            .await
            .unwrap();
        assert_eq!(affine(&context, a), [4096, 0, 0, 10, 0, 4096, 0, 20, 0, 0, 4096, 30]);

        // A quarter turn about y keeps the translation that was there.
        words(&mut context, b, &[0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 7]);
        call(&mut context, 12, args(&[b, 1024])).await.unwrap();
        assert_eq!(affine(&context, b), [0, 0, 4096, 5, 0, 4096, 0, 6, -4096, 0, 0, 7]);

        call(&mut context, 10, args(&[product, a, b])).await.unwrap();
        assert_eq!(affine(&context, product), [0, 0, 4096, 15, 0, 4096, 0, 26, -4096, 0, 0, 37]);
    }

    #[futures_test::test]
    async fn look_at_puts_what_the_camera_faces_in_front_of_it() {
        let mut context = TestContext::new();
        let (this, position, look, up) = (0x1000, 0x1100, 0x1110, 0x1120);
        words(&mut context, position, &[0, 0, -100]);
        words(&mut context, look, &[0, 0, 4096]);
        words(&mut context, up, &[0, 4096, 0]);

        call(&mut context, 15, args(&[this, position, look, up])).await.unwrap();

        // The origin is 100 along the view, straight ahead.
        assert_eq!(affine(&context, this), [-4096, 0, 0, 0, 0, -4096, 0, 0, 0, 0, 4096, 100]);
    }

    /// One of 싸이 디럭스's board tiles, as it hands the list over: a lit,
    /// textured quad with a face normal, then a flush and the end.
    #[test]
    fn a_command_list_quad_is_two_triangles_with_its_coordinates() {
        let list: Vec<i32> = [
            0xfe000001u32,
            0x86000000,
            0x04013201,
            0xfffffe00,
            0,
            0xfffffe00,
            0x200,
            0,
            0xfffffe00,
            0x200,
            0,
            0x200,
            0xfffffe00,
            0,
            0x200,
            0,
            0x1000,
            0,
            0,
            0x40,
            0x3f,
            0x40,
            0x3f,
            0x7f,
            0,
            0x7f,
            0x82000000,
            0x80000000,
        ]
        .iter()
        .map(|&w| w as i32)
        .collect();
        let mut view = render::View {
            affine: [4096, 0, 0, 0, 0, 4096, 0, 0, 0, 0, 4096, 0],
            center: (0, 0),
            near: 1.0,
            far: 32767.0,
            focal: 100.0,
            clip: (0, 0, 240, 320),
            light: None,
        };

        let triangles = command_triangles(&list, &mut view);

        assert_eq!(triangles.len(), 2);
        assert_eq!(triangles[0].points, [[-512.0, 0.0, -512.0], [512.0, 0.0, -512.0], [512.0, 0.0, 512.0]]);
        assert_eq!(triangles[1].points, [[-512.0, 0.0, 512.0], [-512.0, 0.0, -512.0], [512.0, 0.0, 512.0]]);
        assert_eq!(triangles[0].uv, Some([[0.0, 64.0], [63.0, 64.0], [63.0, 127.0]]));
        assert_eq!(triangles[0].normals, Some([[0.0, 4096.0, 0.0]; 3]));
        assert!(triangles[0].double_sided);
    }

    #[test]
    fn a_texture_is_read_top_row_first() {
        // A 2x2 8-bit bitmap, bottom-up: the bottom row is indices 2 3.
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&(14 + 40 + 4 * 4 + 8u32).to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&(14 + 40 + 4 * 4u32).to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&2i32.to_le_bytes());
        bmp.extend_from_slice(&2i32.to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&8u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 16]);
        bmp.extend_from_slice(&4u32.to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        for (b, g, r) in [(0, 0, 0), (0, 0, 255), (0, 255, 0), (255, 0, 0)] {
            bmp.extend_from_slice(&[b, g, r, 0]);
        }
        bmp.extend_from_slice(&[2, 3, 0, 0, 0, 1, 0, 0]);

        let texture = load_texture(&bmp).unwrap();

        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(texture.pixels, [0, 1, 2, 3]);
        assert_eq!(texture.palette[1], [255, 0, 0]);
    }

    /// Bound to a 16x16 target, looking straight down the view at a square of
    /// one colour 100 ahead: the middle of the target is that colour, a corner
    /// is not touched.
    #[futures_test::test]
    async fn a_command_list_lands_on_the_bound_pixels() {
        let mut context = TestContext::new();
        let (graphics, pixels, affine_at, list, descriptor) = (0x1000, 0x2000, 0x1800, 0x3000, 0x3800);

        call(&mut context, 48, args(&[graphics])).await.unwrap();
        call(&mut context, 51, args(&[graphics, 16, 16, 16, pixels])).await.unwrap();
        words(&mut context, affine_at, &[4096, 0, 0, 0, 0, 4096, 0, 0, 0, 0, 4096, 100]);
        call(&mut context, 53, args(&[graphics, affine_at])).await.unwrap();
        call(&mut context, 54, args(&[graphics, 8, 8])).await.unwrap();
        call(&mut context, 55, args(&[graphics, 1, 1000, 1024])).await.unwrap();

        // A quad 20 across with one colour for the command, red.
        let commands: Vec<i32> = [
            0xfe000001u32,
            0x04010400,
            0xfffffff6,
            0xfffffff6,
            0,
            10,
            0xfffffff6,
            0,
            10,
            10,
            0,
            0xfffffff6,
            10,
            0,
            0xff0000,
            0x80000000,
        ]
        .iter()
        .map(|&w| w as i32)
        .collect();
        words(&mut context, list, &commands);
        words(&mut context, descriptor, &[commands.len() as i32, list as i32]);
        call(&mut context, 74, args(&[graphics, descriptor])).await.unwrap();

        let at = |x: u32, y: u32| read_generic::<u16, _>(&context, pixels + (y * 16 + x) * 2).unwrap();
        assert_eq!(at(8, 8), 0xf800);
        assert_eq!(at(0, 0), 0);
    }

    /// Every figure and action table in a directory parses - `WIE_M3D_DIR`,
    /// the files of a title that uses this library.
    #[test]
    #[ignore]
    fn the_files_of_a_title_parse() {
        let dir = std::env::var("WIE_M3D_DIR").unwrap();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }
            let data = std::fs::read(&path).unwrap();
            match path.extension().and_then(|e| e.to_str()) {
                Some("mbac") => {
                    let figure = loader::load_figure(&data).unwrap();
                    std::println!(
                        "{path:?}: {} vertices, {} polygons, {} bones, normals={}",
                        figure.vertices.len(),
                        figure.polygons.len(),
                        figure.bones.len(),
                        figure.normals.is_some()
                    );
                }
                Some("mtra") => {
                    let actions = loader::load_actions(&data).unwrap();
                    std::println!(
                        "{path:?}: {} actions, keyframes {:?}",
                        actions.len(),
                        actions.iter().map(|a| a.keyframes).collect::<Vec<_>>()
                    );
                }
                _ => {}
            }
        }
    }
}
