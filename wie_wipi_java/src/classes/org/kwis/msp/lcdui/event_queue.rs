use alloc::{vec, vec::Vec};

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::MethodAccessFlags;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::Event;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::net::wie::EventQueue as WieEventQueue;

use crate::classes::org::kwis::msp::lcdui::Jlet;

// class org.kwis.msp.lcdui.EventQueue
pub struct EventQueue;

impl EventQueue {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/lcdui/EventQueue",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Lorg/kwis/msp/lcdui/Jlet;)V", Self::init, Default::default()),
                JavaMethodProto::new("getNextEvent", "([I)V", Self::get_next_event, Default::default()),
                JavaMethodProto::new("dispatchEvent", "([I)V", Self::dispatch_event, Default::default()),
                JavaMethodProto::new("postEvent", "([I)Z", Self::post_event, Default::default()),
                JavaMethodProto::new("postEvent", "(I[I)V", Self::post_event_static, MethodAccessFlags::STATIC),
            ],
            fields: vec![JavaFieldProto::new("wieEventQueue", "Lnet/wie/EventQueue;", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<EventQueue>, jlet: ClassInstanceRef<Jlet>) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lcdui.EventQueue::<init>({this:?}, {jlet:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        let wie_event_queue: ClassInstanceRef<WieEventQueue> = jvm
            .invoke_static("net/wie/EventQueue", "getEventQueue", "()Lnet/wie/EventQueue;", ())
            .await?;
        jvm.put_field(&mut this, "wieEventQueue", "Lnet/wie/EventQueue;", wie_event_queue).await?;

        Ok(())
    }

    async fn get_next_event(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        event: ClassInstanceRef<Array<i32>>,
    ) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lcdui.EventQueue::getNextEvent({this:?}, {event:?})");

        let wie_event_queue = jvm.get_field(&this, "wieEventQueue", "Lnet/wie/EventQueue;").await?;
        let _: () = jvm.invoke_virtual(&wie_event_queue, "getNextEvent", "([I)V", (event,)).await?;

        Ok(())
    }

    async fn dispatch_event(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        event: ClassInstanceRef<Array<i32>>,
    ) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lcdui.EventQueue::dispatchEvent({this:?}, {event:?})");

        let wie_event_queue = jvm.get_field(&this, "wieEventQueue", "Lnet/wie/EventQueue;").await?;
        let _: () = jvm.invoke_virtual(&wie_event_queue, "dispatchEvent", "([I)V", (event,)).await?;

        Ok(())
    }

    /// `postEvent(int[] event)` - an event the title hands its own queue:
    /// `[type, param1, param2, ...]`, delivered back to it as a notify event,
    /// the way `MC_grpPostEvent` delivers one on the C side - to the
    /// `JletEventListener`s and to the top card's `notifyEvent(type, param1,
    /// param2)`.
    ///
    /// 판타지맞고 drives its deal with these: entering a hand it posts
    /// `[0x5005, 1, 0, 0]` and then `[0x5005, 2, 0, 0]` from its own thread, and
    /// its card's `notifyEvent` moves the table on when the step it is waiting
    /// for comes back. Dropped, the table sat on its first frame for good.
    async fn post_event(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        event: ClassInstanceRef<Array<i32>>,
    ) -> JvmResult<bool> {
        tracing::debug!("org.kwis.msp.lcdui.EventQueue::postEvent({this:?}, {event:?})");

        Self::post(jvm, context, event).await
    }

    /// `postEvent(int id, int[] event)` - the same, naming the program it is
    /// for. There is one program here.
    async fn post_event_static(jvm: &Jvm, context: &mut WieJvmContext, id: i32, event: ClassInstanceRef<Array<i32>>) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lcdui.EventQueue::postEvent({id}, {event:?})");

        Self::post(jvm, context, event).await?;

        Ok(())
    }

    async fn post(jvm: &Jvm, context: &mut WieJvmContext, event: ClassInstanceRef<Array<i32>>) -> JvmResult<bool> {
        if event.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "").await);
        }

        let length = jvm.array_length(&event).await?;
        let mut values: Vec<i32> = jvm.load_array(&event, 0, length.min(3)).await?;
        values.resize(3, 0);

        tracing::debug!("posted event {values:?}");
        context.system().event_queue().push(Event::Notify {
            r#type: values[0],
            param1: values[1],
            param2: values[2],
        });

        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use jvm::ClassInstanceRef;
    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::{classes::org::kwis::msp::lcdui::EventQueue, get_protos};

    #[test]
    fn test_post_event_is_callable() -> Result<()> {
        run_jvm_test(Box::new([wie_midp::get_protos().into(), get_protos().into()]), |jvm| async move {
            let queue: ClassInstanceRef<EventQueue> = jvm
                .new_class("org/kwis/msp/lcdui/EventQueue", "(Lorg/kwis/msp/lcdui/Jlet;)V", [None.into()])
                .await?
                .into();
            let event = jvm.instantiate_array("I", 4).await?;

            assert!(jvm.invoke_virtual::<_, bool>(&queue, "postEvent", "([I)Z", (event.clone(),)).await?);
            let _: () = jvm
                .invoke_static("org/kwis/msp/lcdui/EventQueue", "postEvent", "(I[I)V", (1, event))
                .await?;

            Ok(())
        })
    }
}
