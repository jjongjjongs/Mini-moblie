use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::m::{MICRO3D_ONE, a3, trig, v3};

// class m.XO_World
//
// SK-VM's 3D renderer. Its trigonometry and camera maths are real, because a
// title does its own geometry with them; the rasterizer and the .mbac/.mtra
// model formats are not, so it keeps what it is told and draws nothing. See the
// module comment.
pub struct XoWorld;

impl XoWorld {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "m/XO_World",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                // The trigonometry and camera maths, answered.
                JavaMethodProto::new("sin", "(I)I", Self::sin, MethodAccessFlags::STATIC),
                JavaMethodProto::new("cos", "(I)I", Self::cos, MethodAccessFlags::STATIC),
                JavaMethodProto::new(
                    "getViewTrans",
                    "(Lm/V3;Lm/V3;Lm/V3;Lm/A3;)V",
                    Self::get_view_trans,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new("rotY", "(ILm/A3;)V", Self::rot_y, MethodAccessFlags::STATIC),
                JavaMethodProto::new("rotZ", "(ILm/A3;)V", Self::rot_z, MethodAccessFlags::STATIC),
                // The renderer, which keeps its state and draws nothing.
                JavaMethodProto::new("loadMBAC", "(Ljava/io/InputStream;)I", Self::load, Default::default()),
                JavaMethodProto::new("loadMTRA", "(Ljava/io/InputStream;)I", Self::load, Default::default()),
                JavaMethodProto::new("loadBMP", "(Ljava/io/InputStream;)I", Self::load, Default::default()),
                JavaMethodProto::new("shareData", "(Lm/XO_World;)V", Self::share_data, Default::default()),
                JavaMethodProto::new(
                    "setVram",
                    "(Ljavax/microedition/lcdui/Graphics;Lm/XO_World;II)V",
                    Self::set_vram,
                    Default::default(),
                ),
                JavaMethodProto::new("setView", "(Lm/A3;IIII)V", Self::set_view, Default::default()),
                JavaMethodProto::new("setClip", "(IIII)V", Self::set_clip, Default::default()),
                JavaMethodProto::new("setPosture", "(II)V", Self::set_posture, Default::default()),
                JavaMethodProto::new("getMaxFrame", "(I)I", Self::get_max_frame, Default::default()),
                JavaMethodProto::new("draw", "(Ljavax/microedition/lcdui/Graphics;)V", Self::draw, Default::default()),
                JavaMethodProto::new("dispose", "()V", Self::dispose, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("m.XO_World::<init>({this:?})");

        jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await
    }

    async fn sin(_: &Jvm, _: &mut WieJvmContext, angle: i32) -> JvmResult<i32> {
        Ok(trig(angle, libm::sin))
    }

    async fn cos(_: &Jvm, _: &mut WieJvmContext, angle: i32) -> JvmResult<i32> {
        Ok(trig(angle, libm::cos))
    }

    /// Build the transform that takes a point into the camera's frame: the three
    /// basis vectors of a look-at, and the camera's own position carried through
    /// them.
    async fn get_view_trans(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        position: ClassInstanceRef<v3::V3>,
        target: ClassInstanceRef<v3::V3>,
        up: ClassInstanceRef<v3::V3>,
        mut out: ClassInstanceRef<a3::A3>,
    ) -> JvmResult<()> {
        tracing::debug!("m.XO_World::getViewTrans({position:?}, {target:?}, {up:?}, {out:?})");

        if position.is_null() || target.is_null() || up.is_null() || out.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "a view takes four objects").await);
        }

        let (eye_x, eye_y, eye_z) = v3::read(jvm, &position).await?;
        let (at_x, at_y, at_z) = v3::read(jvm, &target).await?;
        let (up_x, up_y, up_z) = v3::read(jvm, &up).await?;

        let forward = normalize([(at_x - eye_x) as f64, (at_y - eye_y) as f64, (at_z - eye_z) as f64]);
        let side = normalize(cross(forward, [up_x as f64, up_y as f64, up_z as f64]));
        let true_up = cross(side, forward);

        let rows = [side, true_up, [-forward[0], -forward[1], -forward[2]]];
        let eye = [eye_x as f64, eye_y as f64, eye_z as f64];

        let mut cells = a3::identity();
        for (row, basis) in rows.iter().enumerate() {
            for (column, component) in basis.iter().enumerate() {
                cells[row * 4 + column] = libm::round(component * MICRO3D_ONE as f64) as i32;
            }
            let translation = -(basis[0] * eye[0] + basis[1] * eye[1] + basis[2] * eye[2]);
            cells[row * 4 + 3] = libm::round(translation) as i32;
        }

        a3::write(jvm, &mut out, cells).await
    }

    async fn rot_y(jvm: &Jvm, _: &mut WieJvmContext, angle: i32, target: ClassInstanceRef<a3::A3>) -> JvmResult<()> {
        rotate(jvm, angle, target, Axis::Y).await
    }

    async fn rot_z(jvm: &Jvm, _: &mut WieJvmContext, angle: i32, target: ClassInstanceRef<a3::A3>) -> JvmResult<()> {
        rotate(jvm, angle, target, Axis::Z).await
    }

    /// A loader answers a handle the title discards.
    async fn load(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, stream: ClassInstanceRef<()>) -> JvmResult<i32> {
        tracing::debug!("m.XO_World::load({this:?}, {stream:?})");

        Ok(0)
    }

    async fn share_data(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _other: ClassInstanceRef<Self>) -> JvmResult<()> {
        Ok(())
    }

    async fn set_vram(
        _: &Jvm,
        _: &mut WieJvmContext,
        _this: ClassInstanceRef<Self>,
        _graphics: ClassInstanceRef<()>,
        _world: ClassInstanceRef<Self>,
        _x: i32,
        _y: i32,
    ) -> JvmResult<()> {
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn set_view(
        _: &Jvm,
        _: &mut WieJvmContext,
        _this: ClassInstanceRef<Self>,
        _transform: ClassInstanceRef<a3::A3>,
        _a: i32,
        _b: i32,
        _c: i32,
        _d: i32,
    ) -> JvmResult<()> {
        Ok(())
    }

    async fn set_clip(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _x: i32, _y: i32, _width: i32, _height: i32) -> JvmResult<()> {
        Ok(())
    }

    async fn set_posture(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _a: i32, _b: i32) -> JvmResult<()> {
        Ok(())
    }

    /// An unloaded motion has no frames.
    async fn get_max_frame(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _motion: i32) -> JvmResult<i32> {
        Ok(0)
    }

    async fn draw(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _graphics: ClassInstanceRef<()>) -> JvmResult<()> {
        Ok(())
    }

    async fn dispose(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        Ok(())
    }
}

enum Axis {
    Y,
    Z,
}

/// Fill a transform with a rotation about one axis and no translation, which is
/// how a title turns its camera or its model.
async fn rotate(jvm: &Jvm, angle: i32, mut target: ClassInstanceRef<a3::A3>, axis: Axis) -> JvmResult<()> {
    if target.is_null() {
        return Err(jvm.exception("java/lang/NullPointerException", "rotation target is null").await);
    }

    let sin = trig(angle, libm::sin);
    let cos = trig(angle, libm::cos);

    let mut cells = a3::identity();
    match axis {
        Axis::Y => {
            cells[0] = cos;
            cells[2] = sin;
            cells[8] = -sin;
            cells[10] = cos;
        }
        Axis::Z => {
            cells[0] = cos;
            cells[1] = -sin;
            cells[4] = sin;
            cells[5] = cos;
        }
    }

    a3::write(jvm, &mut target, cells).await
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize(vector: [f64; 3]) -> [f64; 3] {
    let length = libm::sqrt(vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]);
    if length == 0.0 {
        return [0.0, 0.0, 0.0];
    }

    [vector[0] / length, vector[1] / length, vector[2] / length]
}
