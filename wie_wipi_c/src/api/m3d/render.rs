//! Drawing into a 16-bit target, the way Mascot Capsule does in software: a
//! camera affine, a perspective projection about a screen centre, back-face
//! culling and a depth buffer. Figures and command-list primitives both come
//! down to [`Triangle`]s.

use alloc::{vec, vec::Vec};

use super::loader::{self, Fill, material};

/// A texture as a triangle samples it.
pub struct Texels<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
    pub palette: &'a [[u8; 3]],
}

/// Where and how geometry lands on the target.
pub struct View {
    /// The camera: 3x4, rotation in 1/4096, translation in model units.
    pub affine: [i32; 12],
    pub center: (i32, i32),
    pub near: f32,
    pub far: f32,
    /// Focal length in pixels.
    pub focal: f32,
    pub clip: (i32, i32, i32, i32),
    /// The light, when lighting is on: direction (towards the scene, in
    /// camera space), directional and ambient intensity, all in 1/4096.
    pub light: Option<([f32; 3], f32, f32)>,
}

/// The part of the bound pixels a draw touches: `pixels` holds the rows
/// `top..top + rows` and columns `left..left + columns` of a target `width`
/// wide, while `depth` covers all of it.
pub struct Target<'a> {
    pub width: u32,
    pub left: i32,
    pub top: i32,
    pub columns: i32,
    pub pixels: &'a mut [u16],
    pub depth: &'a mut [f32],
}

/// One triangle in model space, before the camera.
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    /// Texture coordinates in texels, or `None` for a flat colour.
    pub uv: Option<[[f32; 2]; 3]>,
    pub color: [u8; 3],
    /// Palette entry 0 is a hole.
    pub color_key: bool,
    pub double_sided: bool,
    /// 0 normal, 2 half, 4 add, 6 subtract.
    pub blend: u32,
    /// Normals per corner, in model space, when the triangle takes light.
    pub normals: Option<[[f32; 3]; 3]>,
}

/// The figure's vertices, and its normals, with its bones (and, if posed,
/// its action) applied.
pub fn pose(figure: &loader::Figure, action: Option<(&loader::Action, i32)>) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let mut matrices: Vec<[f32; 12]> = Vec::with_capacity(figure.bones.len());
    let mut out = Vec::with_capacity(figure.vertices.len());
    let mut normals = Vec::new();
    let mut next = 0;
    for (index, bone) in figure.bones.iter().enumerate() {
        let mut matrix = match bone.parent {
            -1 => bone.matrix,
            parent => loader::mul_affine(&matrices[parent as usize], &bone.matrix),
        };
        if let Some((action, frame)) = action
            && let Some(bone_action) = action.bones.get(index)
        {
            matrix = loader::mul_affine(&matrix, &bone_action.matrix(frame));
        }
        let rotate = |v: &[f32; 3]| {
            [
                v[0] * matrix[0] + v[1] * matrix[1] + v[2] * matrix[2],
                v[0] * matrix[4] + v[1] * matrix[5] + v[2] * matrix[6],
                v[0] * matrix[8] + v[1] * matrix[9] + v[2] * matrix[10],
            ]
        };
        for v in figure.vertices.iter().skip(next).take(bone.vertices) {
            let r = rotate(v);
            out.push([r[0] + matrix[3], r[1] + matrix[7], r[2] + matrix[11]]);
        }
        if let Some(source) = &figure.normals {
            normals.extend(source.iter().skip(next).take(bone.vertices).map(rotate));
        }
        next += bone.vertices;
        matrices.push(matrix);
    }
    (out, normals)
}

/// A posed figure as triangles: quads `a b c d` split into `a b c` and
/// `c b d`, patterns other than 0 only where `patterns` has their bit.
pub fn figure_triangles(figure: &loader::Figure, (posed, normals): &(Vec<[f32; 3]>, Vec<[f32; 3]>), patterns: u32) -> Vec<Triangle> {
    let mut out = Vec::new();
    for polygon in &figure.polygons {
        if polygon.pattern != 0 && polygon.pattern & patterns == 0 {
            continue;
        }
        let corners: &[[usize; 3]] = if polygon.indices.len() == 4 {
            &[[0, 1, 2], [2, 1, 3]]
        } else {
            &[[0, 1, 2]]
        };
        for tri in corners {
            let points = tri.map(|i| posed[polygon.indices[i] as usize]);
            let (uv, color) = match &polygon.fill {
                Fill::Texture { uv } => (Some(tri.map(|i| [uv[i][0] as f32, uv[i][1] as f32])), [0; 3]),
                Fill::Color(color) => (None, *color),
            };
            out.push(Triangle {
                points,
                uv,
                color,
                color_key: polygon.material & material::TRANSPARENT != 0,
                double_sided: polygon.material & material::DOUBLE_FACE != 0,
                blend: polygon.material & material::BLEND_MASK,
                normals: (polygon.material & material::LIGHTING != 0 && !normals.is_empty())
                    .then(|| tri.map(|i| normals[polygon.indices[i] as usize])),
            });
        }
    }
    out
}

fn rgb565([r, g, b]: [u8; 3]) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}

fn from_rgb565(p: u16) -> [u8; 3] {
    let r = ((p >> 11) & 0x1f) as u8;
    let g = ((p >> 5) & 0x3f) as u8;
    let b = (p & 0x1f) as u8;
    [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
}

fn blend(mode: u32, dst: u16, src: [u8; 3]) -> u16 {
    if mode == 0 {
        return rgb565(src);
    }
    let d = from_rgb565(dst);
    rgb565(match mode {
        2 => [
            ((d[0] as u16 + src[0] as u16) / 2) as u8,
            ((d[1] as u16 + src[1] as u16) / 2) as u8,
            ((d[2] as u16 + src[2] as u16) / 2) as u8,
        ],
        4 => [d[0].saturating_add(src[0]), d[1].saturating_add(src[1]), d[2].saturating_add(src[2])],
        _ => [d[0].saturating_sub(src[0]), d[1].saturating_sub(src[1]), d[2].saturating_sub(src[2])],
    })
}

/// A corner on screen: position, `1/z` and the texture coordinate over `z`,
/// for perspective-correct sampling.
#[derive(Clone, Copy, Default)]
struct Corner {
    x: f32,
    y: f32,
    inv_z: f32,
    u_z: f32,
    v_z: f32,
    /// How lit the corner is, 1 being the colour as it is.
    light: f32,
}

/// A triangle on screen, culled and ready to fill.
pub struct Projected<'a> {
    corners: [Corner; 3],
    area: f32,
    triangle: &'a Triangle,
}

/// The triangles that land on screen, and the pixel rectangle they cover
/// within the clip: `(left, top, right, bottom)`, exclusive.
pub fn project<'a>(view: &View, triangles: &'a [Triangle], width: u32, height: u32) -> (Vec<Projected<'a>>, (i32, i32, i32, i32)) {
    let a = &view.affine;
    let to_camera = |v: &[f32; 3]| {
        let row = |r: usize| (v[0] * a[r * 4] as f32 + v[1] * a[r * 4 + 1] as f32 + v[2] * a[r * 4 + 2] as f32) / 4096.0 + a[r * 4 + 3] as f32;
        [row(0), row(1), row(2)]
    };
    let clip = (
        view.clip.0.max(0),
        view.clip.1.max(0),
        (view.clip.0 + view.clip.2).min(width as i32),
        (view.clip.1 + view.clip.3).min(height as i32),
    );

    let mut out = Vec::new();
    let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for triangle in triangles {
        let mut corners = [Corner::default(); 3];
        let mut outside = false;
        for (i, corner) in corners.iter_mut().enumerate() {
            let p = to_camera(&triangle.points[i]);
            if p[2] < view.near || p[2] > view.far {
                outside = true;
                break;
            }
            let inv_z = 1.0 / p[2];
            let (u, v) = triangle.uv.map(|uv| (uv[i][0] + 0.5, uv[i][1] + 0.5)).unwrap_or((0.0, 0.0));
            let light = match (view.light, triangle.normals) {
                (Some((direction, intensity, ambient)), Some(normals)) => {
                    let n = &normals[i];
                    let n = [
                        n[0] * a[0] as f32 + n[1] * a[1] as f32 + n[2] * a[2] as f32,
                        n[0] * a[4] as f32 + n[1] * a[5] as f32 + n[2] * a[6] as f32,
                        n[0] * a[8] as f32 + n[1] * a[9] as f32 + n[2] * a[10] as f32,
                    ];
                    let length = libm::sqrtf(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
                    let lambert = if length == 0.0 {
                        0.0
                    } else {
                        (-(n[0] * direction[0] + n[1] * direction[1] + n[2] * direction[2]) / length).max(0.0)
                    };
                    (ambient + intensity * lambert).min(1.0)
                }
                _ => 1.0,
            };
            *corner = Corner {
                x: view.center.0 as f32 + p[0] * view.focal * inv_z,
                y: view.center.1 as f32 + p[1] * view.focal * inv_z,
                inv_z,
                u_z: u * inv_z,
                v_z: v * inv_z,
                light,
            };
        }
        if outside {
            continue;
        }

        let area = (corners[1].x - corners[0].x) * (corners[2].y - corners[0].y) - (corners[2].x - corners[0].x) * (corners[1].y - corners[0].y);
        if area == 0.0 || (area < 0.0 && !triangle.double_sided) {
            continue;
        }
        let min_x = (corners.iter().map(|p| p.x).fold(f32::MAX, f32::min).floor() as i32).max(clip.0);
        let max_x = (corners.iter().map(|p| p.x).fold(f32::MIN, f32::max).ceil() as i32 + 1).min(clip.2);
        let min_y = (corners.iter().map(|p| p.y).fold(f32::MAX, f32::min).floor() as i32).max(clip.1);
        let max_y = (corners.iter().map(|p| p.y).fold(f32::MIN, f32::max).ceil() as i32 + 1).min(clip.3);
        if min_x >= max_x || min_y >= max_y {
            continue;
        }
        bounds = (bounds.0.min(min_x), bounds.1.min(min_y), bounds.2.max(max_x), bounds.3.max(max_y));
        out.push(Projected { corners, area, triangle });
    }
    (out, bounds)
}

pub fn draw(target: &mut Target, view: &View, triangles: &[Projected], texture: Option<&Texels>) {
    for projected in triangles {
        raster(target, view, &projected.corners, projected.area, projected.triangle, texture);
    }
}

fn raster(target: &mut Target, view: &View, c: &[Corner; 3], area: f32, triangle: &Triangle, texture: Option<&Texels>) {
    let rows = target.pixels.len() as i32 / target.columns.max(1);
    let (cx0, cy0, cx1, cy1) = (
        view.clip.0.max(target.left),
        view.clip.1.max(target.top),
        (view.clip.0 + view.clip.2).min(target.left + target.columns),
        (view.clip.1 + view.clip.3).min(target.top + rows),
    );
    let min_x = (c.iter().map(|p| p.x).fold(f32::MAX, f32::min).floor() as i32).max(cx0);
    let max_x = (c.iter().map(|p| p.x).fold(f32::MIN, f32::max).ceil() as i32).min(cx1 - 1);
    let min_y = (c.iter().map(|p| p.y).fold(f32::MAX, f32::min).floor() as i32).max(cy0);
    let max_y = (c.iter().map(|p| p.y).fold(f32::MIN, f32::max).ceil() as i32).min(cy1 - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }

    let edge = |a: &Corner, b: &Corner, x: f32, y: f32| (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x);

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge(&c[1], &c[2], px, py) / area;
            let w1 = edge(&c[2], &c[0], px, py) / area;
            let w2 = edge(&c[0], &c[1], px, py) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let inv_z = w0 * c[0].inv_z + w1 * c[1].inv_z + w2 * c[2].inv_z;
            let z = 1.0 / inv_z;
            let at = (y as u32 * target.width + x as u32) as usize;
            if z >= target.depth[at] {
                continue;
            }
            let here = ((y - target.top) * target.columns + (x - target.left)) as usize;

            let color = if triangle.uv.is_some() {
                let Some(texture) = texture else {
                    continue;
                };
                let u = (w0 * c[0].u_z + w1 * c[1].u_z + w2 * c[2].u_z) * z;
                let v = (w0 * c[0].v_z + w1 * c[1].v_z + w2 * c[2].v_z) * z;
                let tu = (u.max(0.0) as u32).min(texture.width - 1);
                let tv = (v.max(0.0) as u32).min(texture.height - 1);
                let index = texture.pixels[(tv * texture.width + tu) as usize];
                if triangle.color_key && index == 0 {
                    continue;
                }
                *texture.palette.get(index as usize).unwrap_or(&[0, 0, 0])
            } else {
                triangle.color
            };

            let light = w0 * c[0].light + w1 * c[1].light + w2 * c[2].light;
            let color = if light < 1.0 {
                color.map(|channel| (channel as f32 * light.max(0.0)) as u8)
            } else {
                color
            };
            target.depth[at] = z;
            target.pixels[here] = blend(triangle.blend, target.pixels[here], color);
        }
    }
}

/// A depth buffer as far away as anything can be.
pub fn clear_depth(width: u32, height: u32) -> Vec<f32> {
    vec![f32::MAX; (width * height) as usize]
}
