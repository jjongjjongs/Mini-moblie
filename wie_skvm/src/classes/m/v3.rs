use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class m.V3
//
// A 3D vector. Its three coordinates are public fields a title reads and writes
// directly rather than through accessors.
pub struct V3;

impl V3 {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "m/V3",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("<init>", "(III)V", Self::init_with_coordinates, Default::default()),
                JavaMethodProto::new("set", "(III)V", Self::set, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("x", "I", Default::default()),
                JavaFieldProto::new("y", "I", Default::default()),
                JavaFieldProto::new("z", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("m.V3::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;
        write(jvm, &mut this, 0, 0, 0).await
    }

    async fn init_with_coordinates(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, x: i32, y: i32, z: i32) -> JvmResult<()> {
        tracing::debug!("m.V3::<init>({this:?}, {x}, {y}, {z})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;
        write(jvm, &mut this, x, y, z).await
    }

    async fn set(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, x: i32, y: i32, z: i32) -> JvmResult<()> {
        tracing::debug!("m.V3::set({this:?}, {x}, {y}, {z})");

        write(jvm, &mut this, x, y, z).await
    }
}

/// Read a vector's three coordinates. A null vector reads as the origin, the way
/// the reference answers one.
pub(crate) async fn read<T>(jvm: &Jvm, this: &ClassInstanceRef<T>) -> JvmResult<(i32, i32, i32)> {
    if this.is_null() {
        return Ok((0, 0, 0));
    }

    let x: i32 = jvm.get_field(this, "x", "I").await?;
    let y: i32 = jvm.get_field(this, "y", "I").await?;
    let z: i32 = jvm.get_field(this, "z", "I").await?;

    Ok((x, y, z))
}

/// Write a vector's three coordinates.
pub(crate) async fn write<T>(jvm: &Jvm, this: &mut ClassInstanceRef<T>, x: i32, y: i32, z: i32) -> JvmResult<()> {
    jvm.put_field(this, "x", "I", x).await?;
    jvm.put_field(this, "y", "I", y).await?;
    jvm.put_field(this, "z", "I", z).await?;

    Ok(())
}
