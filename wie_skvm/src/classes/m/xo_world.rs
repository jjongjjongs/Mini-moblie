use alloc::{vec, vec::Vec};

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::io::InputStream;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaIoInputStream};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::javax::microedition::lcdui::{Graphics, Image};

use crate::classes::m::{MICRO3D_ONE, a3, model, trig, v3};

// class m.XO_World
//
// SK-VM's 3D renderer for the Mascot Capsule Micro3D middleware. Its
// trigonometry and camera maths were always real; the model loader and
// rasterizer are now real too, so the `.mbac` model a title loads is drawn with
// its `.bmp` skin. See [`super::model`] for the formats and the rasterizer.
//
// The state a title sets between calls - the loaded model, its skin, the view
// the title projects through, and the posture it selected - lives in this
// object's own fields, so it is freed with the object and needs no registry.
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
                // The renderer.
                JavaMethodProto::new("loadMBAC", "(Ljava/io/InputStream;)I", Self::load_mbac, Default::default()),
                JavaMethodProto::new("loadMTRA", "(Ljava/io/InputStream;)I", Self::load_mtra, Default::default()),
                JavaMethodProto::new("loadBMP", "(Ljava/io/InputStream;)I", Self::load_bmp, Default::default()),
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
            fields: vec![
                // Raw asset bytes, re-parsed at draw time. Storing the bytes
                // keeps every model's state in the object that owns it.
                JavaFieldProto::new("mbacData", "[B", Default::default()),
                JavaFieldProto::new("mtraData", "[B", Default::default()),
                JavaFieldProto::new("bmpData", "[B", Default::default()),
                // The view the title last set through `setView`: twelve 4.12
                // transform cells, a projection scale, and a screen centre.
                JavaFieldProto::new("viewCells", "[I", Default::default()),
                JavaFieldProto::new("viewScale", "I", Default::default()),
                JavaFieldProto::new("viewCx", "I", Default::default()),
                JavaFieldProto::new("viewCy", "I", Default::default()),
                JavaFieldProto::new("hasView", "Z", Default::default()),
                // The posture the title last selected through `setPosture`.
                JavaFieldProto::new("postureAction", "I", Default::default()),
                JavaFieldProto::new("postureFrame", "I", Default::default()),
            ],
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

    /// A loader keeps the bytes it is handed and answers a handle the title
    /// discards. The bytes are parsed at draw time.
    async fn load_mbac(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, stream: ClassInstanceRef<InputStream>) -> JvmResult<i32> {
        tracing::debug!("m.XO_World::loadMBAC({this:?}, {stream:?})");
        Self::store_stream(jvm, this, "mbacData", stream).await?;
        Ok(0)
    }

    async fn load_mtra(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, stream: ClassInstanceRef<InputStream>) -> JvmResult<i32> {
        tracing::debug!("m.XO_World::loadMTRA({this:?}, {stream:?})");
        Self::store_stream(jvm, this, "mtraData", stream).await?;
        Ok(0)
    }

    async fn load_bmp(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, stream: ClassInstanceRef<InputStream>) -> JvmResult<i32> {
        tracing::debug!("m.XO_World::loadBMP({this:?}, {stream:?})");
        Self::store_stream(jvm, this, "bmpData", stream).await?;
        Ok(0)
    }

    /// Read a stream to its end and keep it in a `byte[]` field.
    async fn store_stream(jvm: &Jvm, mut this: ClassInstanceRef<Self>, field: &str, stream: ClassInstanceRef<InputStream>) -> JvmResult<()> {
        if stream.is_null() {
            return Ok(());
        }
        let data = JavaIoInputStream::read_until_end(jvm, &stream).await?;
        let mut array = jvm.instantiate_array("B", data.len() as _).await?;
        jvm.store_array(&mut array, 0, data.into_iter().map(|x| x as i8).collect::<Vec<i8>>())
            .await?;
        jvm.put_field(&mut this, field, "[B", array).await
    }

    /// A model shares another's loaded data: the boss reuses the avatar's mesh
    /// and skin rather than loading its own.
    async fn share_data(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, other: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("m.XO_World::shareData({this:?}, {other:?})");
        if other.is_null() {
            return Ok(());
        }
        for field in ["mbacData", "mtraData", "bmpData"] {
            let value: ClassInstanceRef<Array<i8>> = jvm.get_field(&other, field, "[B").await?;
            jvm.put_field(&mut this, field, "[B", value).await?;
        }
        Ok(())
    }

    async fn set_vram(
        _: &Jvm,
        _: &mut WieJvmContext,
        _this: ClassInstanceRef<Self>,
        _graphics: ClassInstanceRef<Graphics>,
        _world: ClassInstanceRef<Self>,
        _x: i32,
        _y: i32,
    ) -> JvmResult<()> {
        // The double buffer the middleware would render into is the same
        // `Graphics` a title hands `draw`, so there is nothing to set aside here.
        Ok(())
    }

    /// The title's projection: a model-to-camera transform, a projection scale
    /// (passed for x and y alike), and the screen point it is centred on.
    #[allow(clippy::too_many_arguments)]
    async fn set_view(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        transform: ClassInstanceRef<a3::A3>,
        scale: i32,
        _scale_y: i32,
        cx: i32,
        cy: i32,
    ) -> JvmResult<()> {
        let cells = a3::read(jvm, &transform).await?;
        tracing::debug!("m.XO_World::setView({this:?}, scale={scale}, c=({cx},{cy}), cells={cells:?})");
        let mut array = jvm.instantiate_array("I", 12).await?;
        jvm.store_array(&mut array, 0, cells.to_vec()).await?;
        jvm.put_field(&mut this, "viewCells", "[I", array).await?;
        jvm.put_field(&mut this, "viewScale", "I", scale).await?;
        jvm.put_field(&mut this, "viewCx", "I", cx).await?;
        jvm.put_field(&mut this, "viewCy", "I", cy).await?;
        jvm.put_field(&mut this, "hasView", "Z", true).await
    }

    async fn set_clip(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _x: i32, _y: i32, _width: i32, _height: i32) -> JvmResult<()> {
        Ok(())
    }

    async fn set_posture(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, action: i32, frame: i32) -> JvmResult<()> {
        tracing::debug!("m.XO_World::setPosture({this:?}, {action}, {frame})");
        jvm.put_field(&mut this, "postureAction", "I", action).await?;
        jvm.put_field(&mut this, "postureFrame", "I", frame).await
    }

    /// How many frames a motion's action has, in the 16.16 frame units the
    /// title counts in - what the middleware's `getNumFrames` answers. Parsed
    /// from the loaded `.mtra`; no motion, or an action the motion does not
    /// hold, reports none.
    async fn get_max_frame(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, motion: i32) -> JvmResult<i32> {
        tracing::debug!("m.XO_World::getMaxFrame({this:?}, {motion})");
        let Some(mtra) = Self::read_bytes(jvm, &this, "mtraData").await? else {
            return Ok(0);
        };
        let frames = model::Motion::parse(&mtra)
            .and_then(|m| m.num_frames(motion.max(0) as usize))
            .unwrap_or(0);
        Ok(frames)
    }

    /// Draw the loaded model with its skin, through the view the title set.
    async fn draw(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, mut graphics: ClassInstanceRef<Graphics>) -> JvmResult<()> {
        tracing::debug!("m.XO_World::draw({this:?}, {graphics:?})");

        let has_view: bool = jvm.get_field(&this, "hasView", "Z").await?;
        if graphics.is_null() || !has_view {
            return Ok(());
        }

        let Some(mbac) = Self::read_bytes(jvm, &this, "mbacData").await? else {
            return Ok(());
        };
        let Some(m) = model::Model::parse(&mbac) else {
            return Ok(());
        };

        let cells = Self::read_view(jvm, &this).await?;
        let scale: i32 = jvm.get_field(&this, "viewScale", "I").await?;
        let cx: i32 = jvm.get_field(&this, "viewCx", "I").await?;
        let cy: i32 = jvm.get_field(&this, "viewCy", "I").await?;

        let texture = match Self::read_bytes(jvm, &this, "bmpData").await? {
            Some(bmp) => model::parse_texture(&bmp),
            None => None,
        };

        // Pose the model by the posture the title selected, if a motion is
        // loaded and holds that action; otherwise draw the rest pose.
        let action: i32 = jvm.get_field(&this, "postureAction", "I").await?;
        let frame: i32 = jvm.get_field(&this, "postureFrame", "I").await?;
        let motion = match Self::read_bytes(jvm, &this, "mtraData").await? {
            Some(mtra) => model::Motion::parse(&mtra),
            None => None,
        };
        let pose = match &motion {
            Some(mo) => m.animated_pose(mo, action.max(0) as usize, frame),
            None => m.rest_pose(),
        };

        let image = Graphics::image(jvm, &mut graphics).await?;
        let mut canvas = Image::canvas(jvm, &image).await?;

        m.render(&pose, &cells, scale, cx, cy, texture.as_ref(), &mut *canvas);

        Ok(())
    }

    async fn dispose(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        Ok(())
    }

    /// Read a `byte[]` field into bytes, or `None` when it was never set.
    async fn read_bytes(jvm: &Jvm, this: &ClassInstanceRef<Self>, field: &str) -> JvmResult<Option<Vec<u8>>> {
        let array: ClassInstanceRef<Array<i8>> = jvm.get_field(this, field, "[B").await?;
        if array.is_null() {
            return Ok(None);
        }
        let length = jvm.array_length(&array).await?;
        let signed: Vec<i8> = jvm.load_array(&array, 0, length).await?;
        Ok(Some(signed.into_iter().map(|x| x as u8).collect()))
    }

    /// Read the stored view transform, defaulting to the identity.
    async fn read_view(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<[i32; 12]> {
        let array: ClassInstanceRef<Array<i32>> = jvm.get_field(this, "viewCells", "[I").await?;
        if array.is_null() {
            return Ok(a3::identity());
        }
        let values: Vec<i32> = jvm.load_array(&array, 0, 12).await?;
        let mut cells = a3::identity();
        for (cell, value) in cells.iter_mut().zip(values) {
            *cell = value;
        }
        Ok(cells)
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
