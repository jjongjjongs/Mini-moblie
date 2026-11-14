use alloc::{string::ToString, vec};

use futures::TryFutureExt;
use java_class_proto::{JavaClassProto, JavaFieldProto, JavaMethodProto};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::PointerKind;
use wie_core_arm::Allocator;
use wie_midp::classes::javax::microedition::lcdui::Graphics;
use wie_util::ByteWrite;

use super::CletWrapperContext;

/// The clet event a touch arrives as on an LGT handset. See
/// `CletWrapperCard::pointer_notify`.
const LGT_POINTER_EVENT: u32 = 1800;

/// The `{ int x; int y; }` a touch event points at.
const POINT_SIZE: u32 = 8;

// class net.wie.CletWrapperCard
pub struct CletWrapperCard;

impl CletWrapperCard {
    pub fn as_proto() -> JavaClassProto<CletWrapperContext> {
        JavaClassProto {
            name: "net/wie/CletWrapperCard",
            parent_class: Some("org/kwis/msp/lcdui/Card"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(II)V", Self::init, Default::default()),
                JavaMethodProto::new("paint", "(Lorg/kwis/msp/lcdui/Graphics;)V", Self::paint, Default::default()),
                JavaMethodProto::new("keyNotify", "(II)Z", Self::key_notify, Default::default()),
                JavaMethodProto::new("pointerNotify", "(III)Z", Self::pointer_notify, Default::default()),
                JavaMethodProto::new("notifyEvent", "(III)V", Self::notify_event, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("paintClet", "I", Default::default()),
                JavaFieldProto::new("handleCletEvent", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(
        jvm: &Jvm,
        _context: &mut CletWrapperContext,
        mut this: ClassInstanceRef<Self>,
        paint_clet: i32,
        handle_clet_event: i32,
    ) -> JvmResult<()> {
        tracing::debug!("net.wie.CletWrapperCard::<init>({this:?}, {paint_clet:#x}, {handle_clet_event:#x})");

        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lcdui/Card", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "paintClet", "I", paint_clet).await?;
        jvm.put_field(&mut this, "handleCletEvent", "I", handle_clet_event).await?;

        Ok(())
    }

    async fn paint(
        jvm: &Jvm,
        context: &mut CletWrapperContext,
        this: ClassInstanceRef<Self>,
        _graphics: ClassInstanceRef<Graphics>,
    ) -> JvmResult<()> {
        tracing::debug!("net.wie.CletWrapperCard::paint({this:?})");

        let paint_clet: i32 = jvm.get_field(&this, "paintClet", "I").await?;

        context
            .core
            .run_function(paint_clet as _, &[])
            .or_else(async move |x| Err(jvm.exception("net/wie/WieError", &x.to_string()).await))
            .await
    }

    async fn key_notify(jvm: &Jvm, context: &mut CletWrapperContext, this: ClassInstanceRef<Self>, r#type: i32, key: i32) -> JvmResult<bool> {
        tracing::debug!("net.wie.CletWrapperCard::keyNotify({this:?}, {type}, {key})");

        let handle_clet_event: i32 = jvm.get_field(&this, "handleCletEvent", "I").await?;
        let r#type = r#type + 501; // WIPI key types (1/2/3) re-based onto LGT clet event ids (502/503/504)
        let _: () = context
            .core
            .run_function(handle_clet_event as _, &[r#type as _, key as _, 0 as _])
            .or_else(async move |x| Err(jvm.exception("net/wie/WieError", &x.to_string()).await))
            .await?;

        Ok(true)
    }

    /// A touch, as an LGT clet hears one: event 1800, its second argument the
    /// action - 0 down, 1 up, 2 move - and its third a pointer to the point,
    /// `{ int x; int y; }`.
    ///
    /// Read off two LGT touch titles' `handleCletEvent`. 소울게이트 compares
    /// the type against `0xe1 << 3`, turns the action 0/1/2 into its own touch
    /// events 23/24/25 and reads x and y from `[param2]` and `[param2 + 4]`.
    /// 소울세이버 hands any event that is not a key to a helper that does the
    /// same test and the same reads, and calls its own press (with the point),
    /// release (without) and drag (with the point) handlers for 0, 1 and 2.
    async fn pointer_notify(
        jvm: &Jvm,
        context: &mut CletWrapperContext,
        this: ClassInstanceRef<Self>,
        r#type: i32,
        x: i32,
        y: i32,
    ) -> JvmResult<bool> {
        tracing::debug!("net.wie.CletWrapperCard::pointerNotify({this:?}, {type}, {x}, {y})");

        let Some(action) = PointerKind::from_wipi_type(r#type).map(|kind| match kind {
            PointerKind::Pressed => 0u32,
            PointerKind::Released => 1,
            PointerKind::Dragged => 2,
        }) else {
            return Ok(false);
        };

        let handle_clet_event: i32 = jvm.get_field(&this, "handleCletEvent", "I").await?;
        let core = &mut context.core;
        let result: wie_util::Result<()> = async {
            let point = Allocator::alloc(core, POINT_SIZE)?;
            core.write_bytes(point, &[x.to_le_bytes(), y.to_le_bytes()].concat())?;
            let result: wie_util::Result<()> = core.run_function(handle_clet_event as _, &[LGT_POINTER_EVENT, action, point]).await;
            Allocator::free(core, point, POINT_SIZE)?;
            result
        }
        .await;
        if let Err(error) = result {
            return Err(jvm.exception("net/wie/WieError", &error.to_string()).await);
        }

        Ok(true)
    }

    async fn notify_event(
        jvm: &Jvm,
        context: &mut CletWrapperContext,
        this: ClassInstanceRef<Self>,
        r#type: i32,
        param1: i32,
        param2: i32,
    ) -> JvmResult<()> {
        tracing::debug!("net.wie.CletWrapperCard::notifyEvent({this:?}, {type}, {param1}, {param2})");

        let handle_clet_event: i32 = jvm.get_field(&this, "handleCletEvent", "I").await?;
        let _: () = context
            .core
            .run_function(handle_clet_event as _, &[r#type as _, param1 as _, param2 as _])
            .or_else(async move |x| Err(jvm.exception("net/wie/WieError", &x.to_string()).await))
            .await?;

        Ok(())
    }
}
