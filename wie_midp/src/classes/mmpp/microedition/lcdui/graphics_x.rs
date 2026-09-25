use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.microedition.lcdui.GraphicsX
//
// LG WIPI's extended graphics context. On an LG handset the object a Canvas
// hands to `paint` is a `GraphicsX`, and a title reaches its extended drawing
// through a `(GraphicsX) g` downcast. 호국전기이순신 does exactly that in its
// paint path, so the cast has to succeed against the graphics we deliver.
//
// We model it the way LG's own hierarchy does - as the superclass our
// `javax.microedition.lcdui.Graphics` extends - so every graphics we hand out
// is already a `GraphicsX` and the downcast holds. 호국전기이순신 never calls a
// method through the cast, so the class carries nothing but the constructor its
// subclass chains into; the drawing all runs through the standard `Graphics`
// methods it inherits.
pub struct GraphicsX;

impl GraphicsX {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/microedition/lcdui/GraphicsX",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("<init>", "()V", Self::init, Default::default())],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.GraphicsX::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        Ok(())
    }
}
