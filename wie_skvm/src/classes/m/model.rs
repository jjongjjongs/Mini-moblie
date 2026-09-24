//! The renderer behind [`super::XoWorld`]: the SK-VM Mascot Capsule Micro3D
//! model (`.mbac`), its skin (`.bmp`), and a small software rasterizer.
//!
//! This is pure arithmetic with no JVM in it, so it can be unit-tested against a
//! real asset. `XO_World` keeps the raw bytes and the render state in its own
//! Java fields and calls in here to turn them into pixels.
//!
//! ## `.mbac`, version 3
//!
//! The one title that needs this ships the uncompressed version-3 variant, laid
//! out little-endian as:
//!
//! ```text
//!   "MB"  u16 version(=3)  u16 nverts  u16 ntri  u16 nquad  u16 nbones
//!   nverts x { s16 x, y, z }                        -- bone-local positions
//!   ntri  x { u16 material, u16 a,b,c,   u8 uv[6] } -- textured triangles
//!   nquad x { u16 material, u16 a,b,c,d, u8 uv[8] } -- textured quads
//!   nbones x { u16 seg_vertices, s16 parent, s16 matrix[12] }  -- 4.12 fixed
//!   20-byte trailer                                 -- XOR-obfuscated vendor id
//! ```
//!
//! The later bit-packed versions (4/5) that the modern community tools decode
//! are a different, denser encoding; version 3 stores everything as plain
//! arrays, so no bitstream reader is needed here. A bone owns a run of vertices
//! (`seg_vertices` of them, laid out in file order); its `matrix` places those
//! bone-local positions into the model's rest pose, relative to its parent.

use alloc::{vec, vec::Vec};

use wie_backend::canvas::{Canvas, Color};

use crate::classes::m::MICRO3D_ONE;

const MBAC_MAGIC: u16 = 0x424D; // "MB"

/// The extra divisor the title's `setView` scale needs beyond a plain
/// perspective divide, so a model sits at the size the title draws it. The
/// dancer fills much of the bus aisle - head up by the windows, feet on the
/// floor - which the title's own screenshots show; this lands the mesh at that
/// size. Calibrated against 크레이지버스's stage-1 view (`setView` scale 4034, a
/// camera ~740 units back from a mesh ~180 tall); a plain divide draws it
/// several times larger still.
const PROJECTION_DIVISOR: f32 = 6.0;

/// One triangle or quad: vertex indices and per-corner texture coordinates.
struct Face {
    /// 3 for a triangle, 4 for a quad.
    count: usize,
    idx: [usize; 4],
    /// Texture coordinates, one `(u, v)` per corner, in texel units.
    uv: [(u8, u8); 4],
}

/// One bone: the run of vertices it owns and its rest-pose world transform.
struct Bone {
    start: usize,
    end: usize,
    /// 4.12 fixed-point 3x4 transform into model space (rotation over 4096,
    /// translation in raw model units), already composed with the parent's.
    world: [i32; 12],
}

/// A parsed `.mbac` model.
pub struct Model {
    verts: Vec<[i32; 3]>,
    faces: Vec<Face>,
    bones: Vec<Bone>,
}

/// A decoded skin: a small RGB image sampled by a face's texture coordinates.
pub struct Texture {
    width: u32,
    height: u32,
    pixels: Vec<Color>,
}

impl Texture {
    fn sample(&self, u: i32, v: i32) -> Color {
        let x = u.clamp(0, self.width as i32 - 1) as u32;
        let y = v.clamp(0, self.height as i32 - 1) as u32;
        self.pixels[(y * self.width + x) as usize]
    }
}

/// Read a little-endian `u16` at `offset`, or `None` past the end.
fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    u16_at(data, offset).map(|v| v as i16)
}

impl Model {
    /// Parse a version-3 `.mbac`. Returns `None` if the bytes are not a model
    /// this understands, so a bad asset draws nothing rather than failing.
    pub fn parse(data: &[u8]) -> Option<Model> {
        if u16_at(data, 0)? != MBAC_MAGIC || u16_at(data, 2)? != 3 {
            return None;
        }
        let nverts = u16_at(data, 4)? as usize;
        let ntri = u16_at(data, 6)? as usize;
        let nquad = u16_at(data, 8)? as usize;
        let nbones = u16_at(data, 10)? as usize;

        let mut offset = 12;
        let mut verts = Vec::with_capacity(nverts);
        for _ in 0..nverts {
            let x = i16_at(data, offset)? as i32;
            let y = i16_at(data, offset + 2)? as i32;
            let z = i16_at(data, offset + 4)? as i32;
            verts.push([x, y, z]);
            offset += 6;
        }

        let mut faces = Vec::with_capacity(ntri + nquad);
        for _ in 0..ntri {
            // u16 material, then three vertex indices and their (u, v) pairs.
            let a = u16_at(data, offset + 2)? as usize;
            let b = u16_at(data, offset + 4)? as usize;
            let c = u16_at(data, offset + 6)? as usize;
            let uvs = data.get(offset + 8..offset + 14)?;
            faces.push(Face {
                count: 3,
                idx: [a, b, c, 0],
                uv: [(uvs[0], uvs[1]), (uvs[2], uvs[3]), (uvs[4], uvs[5]), (0, 0)],
            });
            offset += 14;
        }
        for _ in 0..nquad {
            let a = u16_at(data, offset + 2)? as usize;
            let b = u16_at(data, offset + 4)? as usize;
            let c = u16_at(data, offset + 6)? as usize;
            let d = u16_at(data, offset + 8)? as usize;
            let uvs = data.get(offset + 10..offset + 18)?;
            faces.push(Face {
                count: 4,
                idx: [a, b, c, d],
                uv: [(uvs[0], uvs[1]), (uvs[2], uvs[3]), (uvs[4], uvs[5]), (uvs[6], uvs[7])],
            });
            offset += 18;
        }

        // Bones. Each owns the next `seg_vertices` vertices in file order, and
        // its rest transform is its local matrix composed onto its parent's.
        let mut bones: Vec<Bone> = Vec::with_capacity(nbones);
        let mut cursor = 0usize;
        for _ in 0..nbones {
            let seg = u16_at(data, offset)? as usize;
            let parent = i16_at(data, offset + 2)?;
            let mut local = [0i32; 12];
            for (cell, value) in local.iter_mut().enumerate() {
                *value = i16_at(data, offset + 4 + cell * 2)? as i32;
            }
            offset += 28;

            let parent_world = if parent >= 0 && (parent as usize) < bones.len() {
                bones[parent as usize].world
            } else {
                crate::classes::m::a3::identity()
            };
            let world = compose(parent_world, local);

            bones.push(Bone {
                start: cursor,
                end: cursor + seg,
                world,
            });
            cursor += seg;
        }

        // Every vertex index a face names must exist.
        for face in &faces {
            for &index in &face.idx[..face.count] {
                if index >= verts.len() {
                    return None;
                }
            }
        }

        Some(Model { verts, faces, bones })
    }

    /// Rest-pose positions: each vertex carried into model space by the world
    /// transform of the bone that owns it.
    fn rest_pose(&self) -> Vec<[i32; 3]> {
        let mut out = self.verts.clone();
        for bone in &self.bones {
            for index in bone.start..bone.end.min(out.len()) {
                out[index] = apply(&bone.world, self.verts[index]);
            }
        }
        out
    }

    /// Draw the model into `canvas`.
    ///
    /// `view` is the 4.12 model-to-camera transform the title built (its `A3`),
    /// `scale` the projection scale and `(cx, cy)` the screen point the camera
    /// looks through - the three arguments the title passes to `setView`.
    pub fn render(&self, view: &[i32; 12], scale: i32, cx: i32, cy: i32, texture: Option<&Texture>, canvas: &mut dyn Canvas) {
        let width = canvas.image().width() as i32;
        let height = canvas.image().height() as i32;
        let rest = self.rest_pose();

        // Project every vertex once. A vertex behind the camera has no valid
        // screen point; its faces are dropped.
        struct Projected {
            x: f32,
            y: f32,
            depth: f32,
            visible: bool,
        }
        let scale = scale as f32;
        let projected: Vec<Projected> = rest
            .iter()
            .map(|&v| {
                let (cxr, cyr, czr) = camera_space(view, v);
                // The title's look-at, as `getViewTrans` builds it, faces down
                // -Z: a point in front of the camera has a negative camera-space
                // z, so the depth to divide by is its magnitude. A point that is
                // not in front has no screen position.
                let depth = -czr;
                if depth <= 1.0 {
                    return Projected {
                        x: 0.0,
                        y: 0.0,
                        depth: 0.0,
                        visible: false,
                    };
                }
                Projected {
                    x: cx as f32 + cxr * scale / (depth * PROJECTION_DIVISOR),
                    y: cy as f32 - cyr * scale / (depth * PROJECTION_DIVISOR),
                    depth,
                    visible: true,
                }
            })
            .collect();

        // Painter's order: farthest faces first. Each quad is two triangles.
        // A drawable triangle: its three vertex indices, their texture
        // coordinates, and the average camera depth it is sorted by.
        type Tri = ([usize; 3], [(u8, u8); 3], f32);
        let mut tris: Vec<Tri> = Vec::new();
        for face in &self.faces {
            let corners: &[usize] = &face.idx[..face.count];
            let split: &[[usize; 3]] = if face.count == 4 { &[[0, 1, 2], [0, 2, 3]] } else { &[[0, 1, 2]] };
            for tri in split {
                let vi = [corners[tri[0]], corners[tri[1]], corners[tri[2]]];
                if !(projected[vi[0]].visible && projected[vi[1]].visible && projected[vi[2]].visible) {
                    continue;
                }
                let uv = [face.uv[tri[0]], face.uv[tri[1]], face.uv[tri[2]]];
                let depth = (projected[vi[0]].depth + projected[vi[1]].depth + projected[vi[2]].depth) / 3.0;
                tris.push((vi, uv, depth));
            }
        }
        tris.sort_by(|a, b| b.2.total_cmp(&a.2));

        if tracing::enabled!(tracing::Level::DEBUG) {
            let mut bounds: Option<(f32, f32, f32, f32)> = None;
            for p in projected.iter().filter(|p| p.visible) {
                let (nx, xx, ny, xy) = bounds.unwrap_or((p.x, p.x, p.y, p.y));
                bounds = Some((nx.min(p.x), xx.max(p.x), ny.min(p.y), xy.max(p.y)));
            }
            tracing::debug!(
                "m.XO_World::draw screen={width}x{height} scale={scale} c=({cx},{cy}) tris={} screen_bbox={bounds:?}",
                tris.len()
            );
        }

        for (vi, uv, _) in &tris {
            let p = [&projected[vi[0]], &projected[vi[1]], &projected[vi[2]]];
            fill_triangle(
                canvas,
                width,
                height,
                [(p[0].x, p[0].y), (p[1].x, p[1].y), (p[2].x, p[2].y)],
                *uv,
                texture,
            );
        }
    }
}

/// `dst = a . b`, a 4.12 fixed-point 3x4 compose (`a`'s translation carried
/// through `b`), matching `m/A3::mul`.
fn compose(a: [i32; 12], b: [i32; 12]) -> [i32; 12] {
    let mut out = [0i32; 12];
    for row in 0..3 {
        for column in 0..3 {
            let mut sum = 0i64;
            for index in 0..3 {
                sum += a[row * 4 + index] as i64 * b[index * 4 + column] as i64;
            }
            out[row * 4 + column] = (sum / MICRO3D_ONE) as i32;
        }
        let mut sum = a[row * 4 + 3] as i64;
        for index in 0..3 {
            sum += a[row * 4 + index] as i64 * b[index * 4 + 3] as i64 / MICRO3D_ONE;
        }
        out[row * 4 + 3] = sum as i32;
    }
    out
}

/// Apply a 4.12 transform to an integer point (rotation over 4096, translation
/// in the point's own units), matching `m/A3::trans`.
fn apply(m: &[i32; 12], v: [i32; 3]) -> [i32; 3] {
    let mut out = [0i32; 3];
    for (row, cell) in out.iter_mut().enumerate() {
        let sum = m[row * 4] as i64 * v[0] as i64 + m[row * 4 + 1] as i64 * v[1] as i64 + m[row * 4 + 2] as i64 * v[2] as i64;
        *cell = (sum / MICRO3D_ONE) as i32 + m[row * 4 + 3];
    }
    out
}

/// A point carried into camera space by the view transform, as floats.
fn camera_space(m: &[i32; 12], v: [i32; 3]) -> (f32, f32, f32) {
    let p = apply(m, v);
    (p[0] as f32, p[1] as f32, p[2] as f32)
}

/// Fill one screen-space triangle, sampling `texture` by affine-interpolated
/// texture coordinates. With no texture a flat mid-grey stands in.
fn fill_triangle(canvas: &mut dyn Canvas, width: i32, height: i32, pts: [(f32, f32); 3], uv: [(u8, u8); 3], texture: Option<&Texture>) {
    let (x0, y0) = pts[0];
    let (x1, y1) = pts[1];
    let (x2, y2) = pts[2];

    // Signed area; a degenerate or back-facing triangle is skipped.
    let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    if area.abs() < 0.5 {
        return;
    }

    let min_x = x0.min(x1).min(x2).floor().max(0.0) as i32;
    let max_x = x0.max(x1).max(x2).ceil().min((width - 1) as f32) as i32;
    let min_y = y0.min(y1).min(y2).floor().max(0.0) as i32;
    let max_y = y0.max(y1).max(y2).ceil().min((height - 1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }

    let inv_area = 1.0 / area;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            // Barycentric weights.
            let w0 = ((x1 - fx) * (y2 - fy) - (x2 - fx) * (y1 - fy)) * inv_area;
            let w1 = ((x2 - fx) * (y0 - fy) - (x0 - fx) * (y2 - fy)) * inv_area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let color = match texture {
                Some(tex) => {
                    let u = w0 * uv[0].0 as f32 + w1 * uv[1].0 as f32 + w2 * uv[2].0 as f32;
                    let v = w0 * uv[0].1 as f32 + w1 * uv[1].1 as f32 + w2 * uv[2].1 as f32;
                    tex.sample(u as i32, v as i32)
                }
                None => Color {
                    a: 255,
                    r: 160,
                    g: 160,
                    b: 170,
                },
            };
            canvas.put_pixel(px, py, color);
        }
    }
}

/// Decode the avatar skin. It is a plain Windows BMP - 8-bit palettised in the
/// stock assets - so the standard decoder handles it, but a hand-rolled reader
/// keeps this free of image-crate assumptions and copes with the 8-bit form the
/// title actually ships.
pub fn parse_texture(data: &[u8]) -> Option<Texture> {
    if data.len() < 54 || &data[0..2] != b"BM" {
        return None;
    }
    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let header_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let raw_height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);
    if width <= 0 || width > 4096 || raw_height == 0 || raw_height.abs() > 4096 {
        return None;
    }
    let top_down = raw_height < 0;
    let height = raw_height.unsigned_abs();
    let width_u = width as u32;

    // Palette follows the info header for the paletted depths.
    let palette_offset = 14 + header_size as usize;
    let mut palette: Vec<Color> = Vec::new();
    if bpp <= 8 {
        let count = (pixel_offset.saturating_sub(palette_offset)) / 4;
        for i in 0..count {
            let e = palette_offset + i * 4;
            let entry = data.get(e..e + 4)?;
            palette.push(Color {
                a: 255,
                r: entry[2],
                g: entry[1],
                b: entry[0],
            });
        }
    }

    // Rows are padded to a multiple of four bytes.
    let row_bytes = match bpp {
        8 => (width_u).next_multiple_of(4) as usize,
        24 => (width_u * 3).next_multiple_of(4) as usize,
        32 => (width_u * 4) as usize,
        _ => return None,
    };

    let mut pixels = vec![Color { a: 255, r: 0, g: 0, b: 0 }; (width_u * height) as usize];
    for row in 0..height {
        let src_row = if top_down { row } else { height - 1 - row };
        let base = pixel_offset + src_row as usize * row_bytes;
        for col in 0..width_u {
            let color = match bpp {
                8 => {
                    let index = *data.get(base + col as usize)? as usize;
                    *palette.get(index).unwrap_or(&Color { a: 255, r: 0, g: 0, b: 0 })
                }
                24 => {
                    let p = data.get(base + col as usize * 3..base + col as usize * 3 + 3)?;
                    Color {
                        a: 255,
                        r: p[2],
                        g: p[1],
                        b: p[0],
                    }
                }
                32 => {
                    let p = data.get(base + col as usize * 4..base + col as usize * 4 + 4)?;
                    Color {
                        a: 255,
                        r: p[2],
                        g: p[1],
                        b: p[0],
                    }
                }
                _ => return None,
            };
            pixels[(row * width_u + col) as usize] = color;
        }
    }

    Some(Texture {
        width: width_u,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    /// A minimal but complete version-3 `.mbac`: a triangle whose three
    /// vertices are owned by one identity bone, so the rest pose is the raw
    /// positions and the whole layout is exercised end to end.
    fn synthetic_mbac() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"MB");
        data.extend_from_slice(&3u16.to_le_bytes()); // version
        data.extend_from_slice(&3u16.to_le_bytes()); // vertices
        data.extend_from_slice(&1u16.to_le_bytes()); // triangles
        data.extend_from_slice(&0u16.to_le_bytes()); // quads
        data.extend_from_slice(&1u16.to_le_bytes()); // bones
        for (x, y, z) in [(0i16, 0i16, 0i16), (10, 0, 0), (0, 10, 0)] {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
            data.extend_from_slice(&z.to_le_bytes());
        }
        // One textured triangle: material 0, indices 0/1/2, uv pairs.
        data.extend_from_slice(&0u16.to_le_bytes());
        for index in [0u16, 1, 2] {
            data.extend_from_slice(&index.to_le_bytes());
        }
        data.extend_from_slice(&[0, 0, 20, 0, 0, 20]);
        // One identity bone owning all three vertices, no parent.
        data.extend_from_slice(&3u16.to_le_bytes());
        data.extend_from_slice(&(-1i16).to_le_bytes());
        for cell in [4096i16, 0, 0, 0, 0, 4096, 0, 0, 0, 0, 4096, 0] {
            data.extend_from_slice(&cell.to_le_bytes());
        }
        data.extend_from_slice(&[0u8; 20]); // trailer
        data
    }

    #[test]
    fn parses_a_version_3_model() {
        let model = Model::parse(&synthetic_mbac()).expect("parse");
        assert_eq!(model.verts.len(), 3);
        assert_eq!(model.faces.len(), 1);
        assert_eq!(model.faces[0].count, 3);
        assert_eq!(&model.faces[0].idx[..3], &[0, 1, 2]);
        assert_eq!(model.bones.len(), 1);
        // An identity bone leaves the positions where they were.
        assert_eq!(model.rest_pose(), model.verts);
    }

    #[test]
    fn rejects_a_wrong_magic() {
        let mut data = synthetic_mbac();
        data[0] = b'X';
        assert!(Model::parse(&data).is_none());
    }

    #[test]
    fn rejects_an_out_of_range_index() {
        let mut data = synthetic_mbac();
        // The triangle's first index sits right after the three vertices; point
        // it past the vertex count.
        let first_index = 12 + 3 * 6 + 2;
        data[first_index] = 99;
        assert!(Model::parse(&data).is_none());
    }

    #[test]
    fn decodes_an_8bit_bmp() {
        // A 2x2 8-bit BMP: a two-entry palette and four indices, bottom-up.
        let mut data = Vec::new();
        data.extend_from_slice(b"BM");
        data.extend_from_slice(&0u32.to_le_bytes()); // file size (unread)
        data.extend_from_slice(&0u32.to_le_bytes()); // reserved
        let pixel_offset = 54u32 + 2 * 4;
        data.extend_from_slice(&pixel_offset.to_le_bytes());
        data.extend_from_slice(&40u32.to_le_bytes()); // header size
        data.extend_from_slice(&2i32.to_le_bytes()); // width
        data.extend_from_slice(&2i32.to_le_bytes()); // height (bottom-up)
        data.extend_from_slice(&1u16.to_le_bytes()); // planes
        data.extend_from_slice(&8u16.to_le_bytes()); // bpp
        data.extend_from_slice(&[0u8; 24]); // rest of the info header
        data.extend_from_slice(&[10, 20, 30, 0]); // palette 0: b,g,r,x
        data.extend_from_slice(&[40, 50, 60, 0]); // palette 1
        data.extend_from_slice(&[0, 1, 0, 0]); // bottom row, padded to 4 bytes
        data.extend_from_slice(&[1, 0, 0, 0]); // top row

        let texture = parse_texture(&data).expect("decode");
        assert_eq!((texture.width, texture.height), (2, 2));
        // Top-left is palette index 1 -> rgb (60, 50, 40).
        let top_left = texture.sample(0, 0);
        assert_eq!((top_left.r, top_left.g, top_left.b), (60, 50, 40));
    }
}
