//! The Java side of a native title: a `Jlet` that starts it and a `Card` that
//! hands it the keys, so the WIPI-Java display and event loop drive a C title
//! the way they drive a Java one. LGT's native titles run the same way - see
//! its `net.wie.CletWrapper`.

use alloc::{string::ToString, vec};

use futures::TryFutureExt;
use java_class_proto::{JavaClassProto, JavaFieldProto, JavaMethodProto};
use java_constants::FieldAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_core_arm::ArmCore;
use wie_midp::classes::javax::microedition::lcdui::Graphics;
use wie_wipi_java::classes::org::kwis::msp::lcdui::Display;

#[derive(Clone)]
pub struct CletContext {
    pub core: ArmCore,
}

/// What the title's `handleCletEvent` is told a key did. WIPI-Java numbers a
/// key's press, release and repeat 1, 2 and 3; the handset's C events are one
/// more - 해적편's handler takes 2 as a press and 3 as a release and drops
/// anything else.
const KEY_EVENT_BASE: i32 = 1;

// class net.wie.SktClet
pub struct SktClet;

impl SktClet {
    pub fn as_proto() -> JavaClassProto<CletContext> {
        JavaClassProto {
            name: "net/wie/SktClet",
            parent_class: Some("org/kwis/msp/lcdui/Jlet"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("startApp", "([Ljava/lang/String;)V", Self::start_app, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("startClet", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("paintClet", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("handleCletEvent", "I", FieldAccessFlags::STATIC),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut CletContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.SktClet::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lcdui/Jlet", "<init>", "()V", ()).await?;

        Ok(())
    }

    async fn start_app(jvm: &Jvm, context: &mut CletContext, this: ClassInstanceRef<Self>, args: ClassInstanceRef<Array<String>>) -> JvmResult<()> {
        tracing::debug!("net.wie.SktClet::startApp({this:?}, {args:?})");

        let start_clet: i32 = jvm.get_static_field("net/wie/SktClet", "startClet", "I").await?;
        let paint_clet: i32 = jvm.get_static_field("net/wie/SktClet", "paintClet", "I").await?;
        let handle_clet_event: i32 = jvm.get_static_field("net/wie/SktClet", "handleCletEvent", "I").await?;

        let display: ClassInstanceRef<Display> = jvm
            .invoke_static("org/kwis/msp/lcdui/Display", "getDefaultDisplay", "()Lorg/kwis/msp/lcdui/Display;", ())
            .await?;
        let card = jvm.new_class("net/wie/SktCletCard", "(II)V", (paint_clet, handle_clet_event)).await?;
        let _: () = jvm.invoke_virtual(&display, "pushCard", "(Lorg/kwis/msp/lcdui/Card;)V", (card,)).await?;

        context
            .core
            .run_function(start_clet as _, &[])
            .or_else(async move |x| Err(jvm.exception("net/wie/WieError", &x.to_string()).await))
            .await
    }
}

// class net.wie.SktCletCard
pub struct SktCletCard;

impl SktCletCard {
    pub fn as_proto() -> JavaClassProto<CletContext> {
        JavaClassProto {
            name: "net/wie/SktCletCard",
            parent_class: Some("org/kwis/msp/lcdui/Card"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(II)V", Self::init, Default::default()),
                JavaMethodProto::new("paint", "(Lorg/kwis/msp/lcdui/Graphics;)V", Self::paint, Default::default()),
                JavaMethodProto::new("keyNotify", "(II)Z", Self::key_notify, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("paintClet", "I", Default::default()),
                JavaFieldProto::new("handleCletEvent", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut CletContext, mut this: ClassInstanceRef<Self>, paint_clet: i32, handle_clet_event: i32) -> JvmResult<()> {
        tracing::debug!("net.wie.SktCletCard::<init>({this:?}, {paint_clet:#x}, {handle_clet_event:#x})");

        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lcdui/Card", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "paintClet", "I", paint_clet).await?;
        jvm.put_field(&mut this, "handleCletEvent", "I", handle_clet_event).await?;

        Ok(())
    }

    async fn paint(jvm: &Jvm, context: &mut CletContext, this: ClassInstanceRef<Self>, _graphics: ClassInstanceRef<Graphics>) -> JvmResult<()> {
        tracing::debug!("net.wie.SktCletCard::paint({this:?})");

        let paint_clet: i32 = jvm.get_field(&this, "paintClet", "I").await?;

        context
            .core
            .run_function(paint_clet as _, &[])
            .or_else(async move |x| Err(jvm.exception("net/wie/WieError", &x.to_string()).await))
            .await
    }

    async fn key_notify(jvm: &Jvm, context: &mut CletContext, this: ClassInstanceRef<Self>, r#type: i32, key: i32) -> JvmResult<bool> {
        tracing::debug!("net.wie.SktCletCard::keyNotify({this:?}, {type}, {key})");

        let handle_clet_event: i32 = jvm.get_field(&this, "handleCletEvent", "I").await?;
        let event = r#type + KEY_EVENT_BASE;
        let _: () = context
            .core
            .run_function(handle_clet_event as _, &[event as _, key as _, 0])
            .or_else(async move |x| Err(jvm.exception("net/wie/WieError", &x.to_string()).await))
            .await?;

        Ok(true)
    }
}
