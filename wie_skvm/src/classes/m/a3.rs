use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::m::{MICRO3D_ONE, v3};

/// The twelve cells in row-major order: three rows, each a rotation triple and a
/// translation.
pub(crate) const CELLS: [&str; 12] = ["m00", "m01", "m02", "m03", "m10", "m11", "m12", "m13", "m20", "m21", "m22", "m23"];

// class m.A3
//
// A 3x4 affine transform: nine rotation cells in fixed point and three
// translation cells in a coordinate's own units.
pub struct A3;

impl A3 {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "m/A3",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("ident", "()V", Self::ident, Default::default()),
                JavaMethodProto::new("set", "(Lm/A3;)V", Self::set, Default::default()),
                JavaMethodProto::new("mul", "(Lm/A3;Lm/A3;)V", Self::mul, Default::default()),
                JavaMethodProto::new("trans", "(Lm/V3;Lm/V3;)V", Self::trans, Default::default()),
            ],
            fields: CELLS.iter().map(|name| JavaFieldProto::new(name, "I", Default::default())).collect(),
            access_flags: Default::default(),
        }
    }

    /// A new transform is the identity, the start a title builds a camera from.
    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("m.A3::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;
        write(jvm, &mut this, identity()).await
    }

    async fn ident(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("m.A3::ident({this:?})");

        write(jvm, &mut this, identity()).await
    }

    async fn set(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, source: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("m.A3::set({this:?}, {source:?})");

        let cells = read(jvm, &source).await?;
        write(jvm, &mut this, cells).await
    }

    /// `mul(a, b)` is `this = a * b`: the rotation renormalized by the fixed
    /// point, and `a`'s translation carried through `b`'s rotation.
    async fn mul(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        left: ClassInstanceRef<Self>,
        right: ClassInstanceRef<Self>,
    ) -> JvmResult<()> {
        tracing::debug!("m.A3::mul({this:?}, {left:?}, {right:?})");

        let left = read(jvm, &left).await?;
        let right = read(jvm, &right).await?;

        let mut product = [0i32; 12];
        for row in 0..3 {
            for column in 0..3 {
                let mut sum = 0i64;
                for index in 0..3 {
                    sum += left[row * 4 + index] as i64 * right[index * 4 + column] as i64;
                }
                product[row * 4 + column] = (sum / MICRO3D_ONE) as i32;
            }
            let mut sum = left[row * 4 + 3] as i64;
            for index in 0..3 {
                sum += left[row * 4 + index] as i64 * right[index * 4 + 3] as i64 / MICRO3D_ONE;
            }
            product[row * 4 + 3] = sum as i32;
        }

        write(jvm, &mut this, product).await
    }

    /// `trans(destination, source)` applies the transform to a point: the
    /// rotation is fixed point and the translation is in the coordinate's own
    /// units.
    async fn trans(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        mut destination: ClassInstanceRef<v3::V3>,
        source: ClassInstanceRef<v3::V3>,
    ) -> JvmResult<()> {
        tracing::debug!("m.A3::trans({this:?}, {destination:?}, {source:?})");

        if destination.is_null() || source.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "a transform takes two vectors").await);
        }

        let transform = read(jvm, &this).await?;
        let (x, y, z) = v3::read(jvm, &source).await?;

        let mut out = [0i32; 3];
        for (row, cell) in out.iter_mut().enumerate() {
            let sum = transform[row * 4] as i64 * x as i64 + transform[row * 4 + 1] as i64 * y as i64 + transform[row * 4 + 2] as i64 * z as i64;
            *cell = (sum / MICRO3D_ONE) as i32 + transform[row * 4 + 3];
        }

        v3::write(jvm, &mut destination, out[0], out[1], out[2]).await
    }
}

pub(crate) fn identity() -> [i32; 12] {
    let one = MICRO3D_ONE as i32;
    [one, 0, 0, 0, 0, one, 0, 0, 0, 0, one, 0]
}

/// Read a transform's twelve cells. A null transform reads as the identity.
pub(crate) async fn read<T>(jvm: &Jvm, this: &ClassInstanceRef<T>) -> JvmResult<[i32; 12]> {
    if this.is_null() {
        return Ok(identity());
    }

    let mut cells = [0i32; 12];
    for (cell, name) in cells.iter_mut().zip(CELLS) {
        *cell = jvm.get_field(this, name, "I").await?;
    }

    Ok(cells)
}

/// Write a transform's twelve cells.
pub(crate) async fn write<T>(jvm: &Jvm, this: &mut ClassInstanceRef<T>, cells: [i32; 12]) -> JvmResult<()> {
    for (value, name) in cells.into_iter().zip(CELLS) {
        jvm.put_field(this, name, "I", value).await?;
    }

    Ok(())
}
