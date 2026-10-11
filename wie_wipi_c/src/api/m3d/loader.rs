//! The Mascot Capsule V3 data formats: figures (`.mbac`) and action tables
//! (`.mtra`).
//!
//! The layouts follow J2ME-Loader's `micro3d` reader (Apache-2.0,
//! Yury Kharchenko), which is the only public account of them.

use alloc::{vec, vec::Vec};

use wie_util::{Result, WieError};

/// A polygon's material bits, as the figure stores them.
pub mod material {
    pub const TRANSPARENT: u32 = 1;
    pub const BLEND_MASK: u32 = 6;
    pub const DOUBLE_FACE: u32 = 16;
    pub const LIGHTING: u32 = 32;
}

/// What a polygon is filled with.
#[derive(Clone, Debug)]
pub enum Fill {
    /// One colour.
    Color([u8; 3]),
    /// A texture, one coordinate per corner.
    Texture { uv: Vec<[u8; 2]> },
}

#[derive(Clone, Debug)]
pub struct Polygon {
    pub material: u32,
    /// Three corners or four; a quad is `a b c d` with `a b c` and `c b d` as
    /// its triangles.
    pub indices: Vec<u16>,
    pub fill: Fill,
    /// The pattern bit this polygon belongs to; 0 is always drawn.
    pub pattern: u32,
}

#[derive(Clone, Debug)]
pub struct Bone {
    /// How many of the vertices, in order, this bone moves.
    pub vertices: usize,
    pub parent: i32,
    /// 3x4, rotation in 1/4096, translation in model units.
    pub matrix: [f32; 12],
}

#[derive(Clone, Debug)]
pub struct Figure {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub polygons: Vec<Polygon>,
    pub bones: Vec<Bone>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    cache: u32,
    cached: u32,
}

fn format_error(what: &str) -> WieError {
    WieError::FatalError(alloc::format!("m3d: {what}"))
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            cache: 0,
            cached: 0,
        }
    }

    fn u8(&mut self) -> Result<u8> {
        let value = *self.data.get(self.pos).ok_or_else(|| format_error("unexpected end of data"))?;
        self.pos += 1;
        Ok(value)
    }

    fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes([self.u8()?, self.u8()?, self.u8()?, self.u8()?]))
    }

    fn ubits(&mut self, size: u32) -> Result<u32> {
        if size == 0 {
            return Ok(0);
        }
        if size > 25 {
            return Err(format_error("bit field too wide"));
        }
        while size > self.cached {
            self.cache |= (self.u8()? as u32) << self.cached;
            self.cached += 8;
        }
        let result = self.cache & ((1u32 << size) - 1);
        self.cached -= size;
        self.cache >>= size;
        Ok(result)
    }

    fn bits(&mut self, size: u32) -> Result<i32> {
        let shift = 32 - size;
        Ok(((self.ubits(size)? << shift) as i32) >> shift)
    }

    fn clear_cache(&mut self) {
        self.cache = 0;
        self.cached = 0;
    }
}

const TO_FLOAT: f32 = 1.0 / 4096.0;

fn read_matrix(reader: &mut Reader) -> Result<[f32; 12]> {
    let mut m = [0f32; 12];
    for (i, value) in m.iter_mut().enumerate() {
        let raw = reader.i16()? as f32;
        *value = if i % 4 == 3 { raw } else { raw * TO_FLOAT };
    }
    Ok(m)
}

pub fn load_figure(data: &[u8]) -> Result<Figure> {
    let mut r = Reader::new(data);
    if r.u8()? != b'M' || r.u8()? != b'B' {
        return Err(format_error("not an MBAC file"));
    }
    let version = r.u8()?;
    if r.u8()? != 0 || !(2..=5).contains(&version) {
        return Err(format_error("unsupported MBAC version"));
    }
    let (vertex_format, normal_format, polygon_format, bone_format) = if version > 3 {
        (r.u8()?, r.u8()?, r.u8()?, r.u8()?)
    } else {
        (1, 0, 1, 1)
    };
    if bone_format != 1 {
        return Err(format_error("unexpected bone format"));
    }

    let num_vertices = r.u16()? as usize;
    let num_poly_t3 = r.u16()? as usize;
    let num_poly_t4 = r.u16()? as usize;
    let num_bones = r.u16()? as usize;

    let (num_poly_c3, num_poly_c4, num_textures, num_patterns, num_colors) = if polygon_format < 3 {
        (0, 0, 1, 1, 0)
    } else {
        (
            r.u16()? as usize,
            r.u16()? as usize,
            r.u16()? as usize,
            r.u16()? as usize,
            r.u16()? as usize,
        )
    };
    if num_vertices > 21845 || num_textures > 16 || num_patterns > 33 || num_colors > 256 {
        return Err(format_error("MBAC counts out of range"));
    }

    // Per pattern: (colour triangles, colour quads), then per texture
    // (triangles, quads).
    let mut patterns = vec![vec![[0usize; 2]; num_textures + 1]; num_patterns];
    if version == 5 {
        for pattern in patterns.iter_mut() {
            for counts in pattern.iter_mut() {
                counts[0] = r.u16()? as usize;
                counts[1] = r.u16()? as usize;
            }
        }
    } else {
        patterns[0][0] = [num_poly_c3, num_poly_c4];
        patterns[0][1] = [num_poly_t3, num_poly_t4];
    }

    let mut vertices = Vec::with_capacity(num_vertices);
    match vertex_format {
        1 => {
            for _ in 0..num_vertices {
                vertices.push([r.i16()? as f32, r.i16()? as f32, r.i16()? as f32]);
            }
        }
        2 => {
            const SIZES: [u32; 4] = [8, 10, 13, 16];
            while vertices.len() < num_vertices {
                let chunk = r.ubits(8)?;
                let size = SIZES[(chunk >> 6) as usize];
                let count = (chunk & 0x3f) as usize + 1;
                if vertices.len() + count > num_vertices {
                    return Err(format_error("vertex data past the vertex count"));
                }
                for _ in 0..count {
                    vertices.push([r.bits(size)? as f32, r.bits(size)? as f32, r.bits(size)? as f32]);
                }
            }
        }
        _ => return Err(format_error("unexpected vertex format")),
    }
    r.clear_cache();

    let normals = match normal_format {
        0 => None,
        1 => {
            let mut normals = Vec::with_capacity(num_vertices);
            for _ in 0..num_vertices {
                normals.push([r.i16()? as f32, r.i16()? as f32, r.i16()? as f32]);
            }
            Some(normals)
        }
        2 => {
            const POOL: [i32; 8] = [0, 0, 64, 0, 0, -64, 0, 0];
            let mut normals = Vec::with_capacity(num_vertices);
            for _ in 0..num_vertices {
                let x = r.ubits(7)?;
                let (x, y, z) = if x == 64 {
                    let kind = r.ubits(3)? as usize;
                    if kind > 5 {
                        return Err(format_error("normal read error"));
                    }
                    (POOL[kind + 2], POOL[kind + 1], POOL[kind])
                } else {
                    let x = ((x << 25) as i32) >> 25;
                    let y = ((r.ubits(7)? << 25) as i32) >> 25;
                    let sign = r.ubits(1)?;
                    let dq = 4096 - x * x - y * y;
                    let mut z = if dq > 0 { libm::roundf(libm::sqrtf(dq as f32)) as i32 } else { 0 };
                    if sign == 1 {
                        z = -z;
                    }
                    (x, y, z)
                };
                // Stored on a scale of 64; kept on the 4096 of everything else.
                normals.push([x as f32 * 64.0, y as f32 * 64.0, z as f32 * 64.0]);
            }
            Some(normals)
        }
        _ => return Err(format_error("unsupported normal format")),
    };
    r.clear_cache();

    let check = |indices: &[u32]| -> Result<Vec<u16>> {
        if indices.iter().any(|&i| i as usize >= num_vertices) {
            return Err(format_error("vertex index past the vertex count"));
        }
        Ok(indices.iter().map(|&i| i as u16).collect())
    };

    let mut polygons_c = Vec::with_capacity(num_poly_c3 + num_poly_c4);
    if num_poly_c3 + num_poly_c4 > 0 {
        let material_bits = r.u8()? as u32;
        let index_bits = r.u8()? as u32;
        let color_bits = r.u8()? as u32;
        let color_id_bits = r.u8()? as u32;
        let _ = r.u8()?;
        let mut colors = Vec::with_capacity(num_colors);
        for _ in 0..num_colors {
            colors.push([r.ubits(color_bits)? as u8, r.ubits(color_bits)? as u8, r.ubits(color_bits)? as u8]);
        }
        for i in 0..num_poly_c3 + num_poly_c4 {
            let material = r.ubits(material_bits)? << 1;
            let corners = if i < num_poly_c3 { 3 } else { 4 };
            let mut raw = [0u32; 4];
            for slot in raw.iter_mut().take(corners) {
                *slot = r.ubits(index_bits)?;
            }
            let color = *colors.get(r.ubits(color_id_bits)? as usize).unwrap_or(&[0, 0, 0]);
            polygons_c.push(Polygon {
                material,
                indices: check(&raw[..corners])?,
                fill: Fill::Color(color),
                pattern: 0,
            });
        }
    }

    let mut polygons_t = Vec::with_capacity(num_poly_t3 + num_poly_t4);
    if num_poly_t3 + num_poly_t4 > 0 {
        let (material_bits, index_bits, uv_bits) = match polygon_format {
            1 => (0, 0, 0),
            2 => (r.u8()? as u32, r.u8()? as u32, 7),
            3 => {
                let m = r.ubits(8)?;
                let v = r.ubits(8)?;
                let u = r.ubits(8)?;
                let _ = r.ubits(8)?;
                (m, v, u)
            }
            _ => return Err(format_error("unexpected polygon format")),
        };
        for i in 0..num_poly_t3 + num_poly_t4 {
            let corners = if i < num_poly_t3 { 3 } else { 4 };
            let mut raw = [0u32; 4];
            let mut uv = Vec::with_capacity(corners);
            let material;
            if polygon_format == 1 {
                let m = r.u16()? as u32;
                for slot in raw.iter_mut().take(corners) {
                    *slot = r.u16()? as u32;
                }
                for _ in 0..corners {
                    uv.push([r.i8()? as u8, r.i8()? as u8]);
                }
                material = ((m & 4) << 2) | ((m & 2) >> 1);
            } else {
                material = r.ubits(material_bits)?;
                for slot in raw.iter_mut().take(corners) {
                    *slot = r.ubits(index_bits)?;
                }
                for _ in 0..corners {
                    uv.push([r.ubits(uv_bits)? as u8, r.ubits(uv_bits)? as u8]);
                }
            }
            polygons_t.push(Polygon {
                material,
                indices: check(&raw[..corners])?,
                fill: Fill::Texture { uv },
                pattern: 0,
            });
        }
    }
    r.clear_cache();

    // Hand out the pattern bits in the order the counts give: colour
    // triangles and quads first, then each texture's.
    let (mut c3, mut c4, mut t3, mut t4) = (0, num_poly_c3, 0, num_poly_t3);
    for (index, pattern) in patterns.iter().enumerate() {
        let bit = if index == 0 { 0 } else { 1u32 << index };
        for (next, count) in [(&mut c3, pattern[0][0]), (&mut c4, pattern[0][1])] {
            for _ in 0..count {
                if let Some(p) = polygons_c.get_mut(*next) {
                    p.pattern = bit;
                }
                *next += 1;
            }
        }
        for counts in pattern.iter().skip(1) {
            for (next, count) in [(&mut t3, counts[0]), (&mut t4, counts[1])] {
                for _ in 0..count {
                    if let Some(p) = polygons_t.get_mut(*next) {
                        p.pattern = bit;
                    }
                    *next += 1;
                }
            }
        }
    }

    let mut bones = Vec::with_capacity(num_bones);
    let mut bone_vertices = 0;
    for _ in 0..num_bones {
        let vertices = r.u16()? as usize;
        let parent = r.i16()? as i32;
        if parent < -1 {
            return Err(format_error("negative bone parent"));
        }
        bones.push(Bone {
            vertices,
            parent,
            matrix: read_matrix(&mut r)?,
        });
        bone_vertices += vertices;
    }
    if bone_vertices != num_vertices {
        return Err(format_error("bones do not cover the vertices"));
    }

    let mut polygons = polygons_c;
    polygons.extend(polygons_t);

    Ok(Figure {
        vertices,
        normals,
        polygons,
        bones,
    })
}

/// Keyframed values for one channel of a bone.
#[derive(Clone, Debug, Default)]
pub struct Track {
    pub keys: Vec<i32>,
    pub values: Vec<[f32; 3]>,
}

impl Track {
    fn read(r: &mut Reader, scale: f32) -> Result<Self> {
        let count = r.u16()? as usize;
        let mut track = Track::default();
        for _ in 0..count {
            track.keys.push(r.u16()? as i32);
            track
                .values
                .push([r.i16()? as f32 * scale, r.i16()? as f32 * scale, r.i16()? as f32 * scale]);
        }
        Ok(track)
    }

    fn single(value: [f32; 3]) -> Self {
        Self {
            keys: vec![0],
            values: vec![value],
        }
    }

    /// The value at `frame` keyframes, interpolated between the keys around it.
    pub fn get(&self, frame: f32) -> [f32; 3] {
        let Some(&last) = self.keys.last() else {
            return [0.0; 3];
        };
        if frame >= last as f32 {
            return self.values[self.values.len() - 1];
        }
        for i in (0..self.keys.len() - 1).rev() {
            let key = self.keys[i] as f32;
            if key > frame {
                continue;
            }
            let a = self.values[i];
            if key == frame {
                return a;
            }
            let b = self.values[i + 1];
            let t = (frame - key) / (self.keys[i + 1] as f32 - key);
            return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        }
        self.values[0]
    }
}

#[derive(Clone, Debug)]
pub enum BoneAction {
    Matrix([f32; 12]),
    Identity,
    Animated {
        translate: Option<Track>,
        scale: Option<Track>,
        rotate: Option<Track>,
        /// Roll in radians, carried in the first component.
        roll: Option<Track>,
    },
}

#[derive(Clone, Debug)]
pub struct Action {
    pub keyframes: u32,
    pub bones: Vec<BoneAction>,
}

const TO_RADIANS: f32 = core::f32::consts::PI / 2048.0;

fn read_roll(r: &mut Reader) -> Result<Track> {
    let count = r.u16()? as usize;
    let mut track = Track::default();
    for _ in 0..count {
        track.keys.push(r.u16()? as i32);
        track.values.push([r.i16()? as f32 * TO_RADIANS, 0.0, 0.0]);
    }
    Ok(track)
}

pub fn load_actions(data: &[u8]) -> Result<Vec<Action>> {
    let mut r = Reader::new(data);
    if r.u8()? != b'M' || r.u8()? != b'T' {
        return Err(format_error("not an MTRA file"));
    }
    let version = r.u8()?;
    if r.u8()? != 0 || !(2..=5).contains(&version) {
        return Err(format_error("unsupported MTRA version"));
    }
    let num_actions = r.u16()? as usize;
    let num_bones = r.u16()? as usize;
    for _ in 0..8 {
        r.u16()?;
    }
    let _data_size = r.i32()?;

    let mut actions = Vec::with_capacity(num_actions);
    for _ in 0..num_actions {
        let keyframes = r.u16()? as u32;
        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            let kind = r.u8()?;
            bones.push(match kind {
                0 => BoneAction::Matrix(read_matrix(&mut r)?),
                1 => BoneAction::Identity,
                2 => {
                    let translate = Track::read(&mut r, 1.0)?;
                    let scale = Track::read(&mut r, TO_FLOAT)?;
                    let rotate = Track::read(&mut r, 1.0)?;
                    let roll = read_roll(&mut r)?;
                    BoneAction::Animated {
                        translate: Some(translate),
                        scale: Some(scale),
                        rotate: Some(rotate),
                        roll: Some(roll),
                    }
                }
                3 => {
                    let translate = Track::single([r.i16()? as f32, r.i16()? as f32, r.i16()? as f32]);
                    let rotate = Track::read(&mut r, 1.0)?;
                    let roll = Track::single([r.i16()? as f32 * TO_RADIANS, 0.0, 0.0]);
                    BoneAction::Animated {
                        translate: Some(translate),
                        scale: None,
                        rotate: Some(rotate),
                        roll: Some(roll),
                    }
                }
                4 => {
                    let rotate = Track::read(&mut r, 1.0)?;
                    let roll = read_roll(&mut r)?;
                    BoneAction::Animated {
                        translate: None,
                        scale: None,
                        rotate: Some(rotate),
                        roll: Some(roll),
                    }
                }
                5 => BoneAction::Animated {
                    translate: None,
                    scale: None,
                    rotate: Some(Track::read(&mut r, 1.0)?),
                    roll: None,
                },
                6 => {
                    let translate = Track::read(&mut r, 1.0)?;
                    let rotate = Track::read(&mut r, 1.0)?;
                    let roll = read_roll(&mut r)?;
                    BoneAction::Animated {
                        translate: Some(translate),
                        scale: None,
                        rotate: Some(rotate),
                        roll: Some(roll),
                    }
                }
                _ => return Err(format_error("unsupported bone animation type")),
            });
        }
        // Version 5 adds which pattern bits come on at which frames; nothing
        // here draws patterns by frame yet, so they are stepped over.
        if version >= 5 {
            let count = r.u16()? as usize;
            for _ in 0..count {
                r.u16()?;
                r.i32()?;
            }
        }
        actions.push(Action { keyframes, bones });
    }

    Ok(actions)
}

const IDENTITY: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// Rotates `m` so its z axis points along `(x, y, z)`.
fn rotate_to(m: &mut [f32; 12], x: f32, y: f32, z: f32) {
    if x == 0.0 && y == 0.0 {
        if z < 0.0 {
            m[5] = -1.0;
            m[10] = -1.0;
        }
        return;
    }
    let rld = 1.0 / libm::sqrtf(x * x + y * y + z * z);
    let (x, y, z) = (x * rld, y * rld, z * rld);
    let (mut rx, mut ry) = (-y, x);
    let rls = 1.0 / libm::sqrtf(rx * rx + ry * ry);
    rx *= rls;
    ry *= rls;
    let sin = libm::sqrtf((1.0 - z * z).max(0.0));
    let nc = 1.0 - z;
    let (xy, xs, ys) = (rx * ry, rx * sin, ry * sin);
    m[0] = rx * rx * nc + z;
    m[1] = xy * nc;
    m[2] = ys;
    m[4] = xy * nc;
    m[5] = ry * ry * nc + z;
    m[6] = -xs;
    m[8] = -ys;
    m[9] = xs;
    m[10] = z;
}

fn roll(m: &mut [f32; 12], angle: f32) {
    if angle == 0.0 {
        return;
    }
    let (s, c) = (libm::sinf(angle), libm::cosf(angle));
    for row in 0..3 {
        let (a, b) = (m[row * 4], m[row * 4 + 1]);
        m[row * 4] = a * c + b * s;
        m[row * 4 + 1] = b * c - a * s;
    }
}

impl BoneAction {
    /// The bone's matrix at `frame`, in 1/65536 keyframes.
    pub fn matrix(&self, frame: i32) -> [f32; 12] {
        let kf = frame as f32 / 65536.0;
        match self {
            BoneAction::Matrix(m) => *m,
            BoneAction::Identity => IDENTITY,
            BoneAction::Animated {
                translate,
                scale,
                rotate,
                roll: roll_track,
            } => {
                let mut m = IDENTITY;
                if let Some(t) = translate {
                    let v = t.get(kf);
                    m[3] = v[0];
                    m[7] = v[1];
                    m[11] = v[2];
                }
                if let Some(r) = rotate {
                    let v = r.get(kf);
                    rotate_to(&mut m, v[0], v[1], v[2]);
                }
                if let Some(r) = roll_track {
                    roll(&mut m, r.get(kf)[0]);
                }
                if let Some(s) = scale {
                    let v = s.get(kf);
                    for row in 0..3 {
                        m[row * 4] *= v[0];
                        m[row * 4 + 1] *= v[1];
                        m[row * 4 + 2] *= v[2];
                    }
                }
                m
            }
        }
    }
}

/// `a * b`, both 3x4 affine.
pub fn mul_affine(a: &[f32; 12], b: &[f32; 12]) -> [f32; 12] {
    let mut out = [0f32; 12];
    for row in 0..3 {
        for col in 0..4 {
            let mut v = a[row * 4] * b[col] + a[row * 4 + 1] * b[4 + col] + a[row * 4 + 2] * b[8 + col];
            if col == 3 {
                v += a[row * 4 + 3];
            }
            out[row * 4 + col] = v;
        }
    }
    out
}
