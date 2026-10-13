use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use crate::classes::org::kwis::msp::lcdui::Image;
use crate::classes::org::kwis::msp::lwc::ActionListener;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class org.kwis.msp.lwc.ButtonComponent
//
// A focusable label. The WIPI UI framework builds a form's buttons from this,
// and 두뇌게임Q's name-entry screen is one such form - without the class its
// `q.paint` dies with NoClassDefFoundError the moment the form is drawn. It
// extends LabelComponent, so the label, font, image, preferred-size and
// text-wrapping behaviour are all inherited; what it adds is that it can take
// focus (mask bit 0x4) and that pressing the select key fires an
// `ActionListener`.
//
// The focus frame drawn in `paintContent` is this runtime's own - the reference
// decorator's exact border geometry is not reproduced - so it is kept to a
// single rectangle around the inherited label, red while focused like the other
// focusable components here.
pub struct ButtonComponent;

impl ButtonComponent {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/lwc/ButtonComponent",
            parent_class: Some("org/kwis/msp/lwc/LabelComponent"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("<init>", "(Ljava/lang/String;)V", Self::init_label, Default::default()),
                JavaMethodProto::new(
                    "<init>",
                    "(Ljava/lang/String;Lorg/kwis/msp/lcdui/Image;)V",
                    Self::init_label_image,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "<init>",
                    "(Ljava/lang/String;Ljava/lang/String;)V",
                    Self::init_label_resource,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "setActionListener",
                    "(Lorg/kwis/msp/lwc/ActionListener;Ljava/lang/Object;)V",
                    Self::set_action_listener,
                    Default::default(),
                ),
                JavaMethodProto::new("keyNotify", "(II)Z", Self::key_notify, Default::default()),
                JavaMethodProto::new(
                    "paintContent",
                    "(Lorg/kwis/msp/lcdui/Graphics;)V",
                    Self::paint_content,
                    Default::default(),
                ),
            ],
            fields: vec![
                JavaFieldProto::new("__wieButtonActionListener", "Lorg/kwis/msp/lwc/ActionListener;", Default::default()),
                JavaFieldProto::new("__wieButtonActionData", "Ljava/lang/Object;", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    /// Makes the button focusable, the one thing it adds to a plain label.
    async fn make_focusable(jvm: &Jvm, this: &mut ClassInstanceRef<Self>) -> JvmResult<()> {
        let mask: i32 = jvm.get_field(this, "mask", "I").await?;
        jvm.put_field(this, "mask", "I", mask | 0x4).await?;

        Ok(())
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lwc/LabelComponent", "<init>", "()V", ()).await?;

        Self::make_focusable(jvm, &mut this).await
    }

    async fn init_label(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, label: ClassInstanceRef<String>) -> JvmResult<()> {
        let _: () = jvm
            .invoke_special(&this, "org/kwis/msp/lwc/LabelComponent", "<init>", "(Ljava/lang/String;)V", (label,))
            .await?;

        Self::make_focusable(jvm, &mut this).await
    }

    async fn init_label_image(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        label: ClassInstanceRef<String>,
        image: ClassInstanceRef<Image>,
    ) -> JvmResult<()> {
        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/LabelComponent",
                "<init>",
                "(Ljava/lang/String;Lorg/kwis/msp/lcdui/Image;)V",
                (label, image),
            )
            .await?;

        Self::make_focusable(jvm, &mut this).await
    }

    async fn init_label_resource(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        label: ClassInstanceRef<String>,
        resource: ClassInstanceRef<String>,
    ) -> JvmResult<()> {
        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/LabelComponent",
                "<init>",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                (label, resource),
            )
            .await?;

        Self::make_focusable(jvm, &mut this).await
    }

    async fn set_action_listener(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        listener: ClassInstanceRef<ActionListener>,
        data: ClassInstanceRef<()>,
    ) -> JvmResult<()> {
        jvm.put_field(&mut this, "__wieButtonActionListener", "Lorg/kwis/msp/lwc/ActionListener;", listener)
            .await?;
        jvm.put_field(&mut this, "__wieButtonActionData", "Ljava/lang/Object;", data).await?;

        Ok(())
    }

    async fn key_notify(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, event_type: i32, key: i32) -> JvmResult<bool> {
        // Key-press events arrive as type 2, the same type the sibling focusable
        // components (ScrollbarComponent, ContainerComponent) act on.
        if event_type != 2 {
            return Ok(false);
        }

        let action: i32 = jvm.invoke_static("org/kwis/msp/lcdui/Display", "getGameAction", "(I)I", (key,)).await?;

        // 8 = FIRE (the select / OK key). Only that fires the button; the pad
        // directions are left for the container to traverse with.
        if action != 8 {
            return Ok(false);
        }

        let listener: ClassInstanceRef<ActionListener> = jvm
            .get_field(&this, "__wieButtonActionListener", "Lorg/kwis/msp/lwc/ActionListener;")
            .await?;

        if listener.is_null() {
            return Ok(false);
        }

        // ActionListener.action(Component source, Object data). The button is
        // the source; the data is whatever was handed to setActionListener -
        // the game uses it to tell its buttons apart.
        let data: ClassInstanceRef<()> = jvm.get_field(&this, "__wieButtonActionData", "Ljava/lang/Object;").await?;
        let _: () = jvm
            .invoke_virtual(
                &listener,
                "action",
                "(Lorg/kwis/msp/lwc/Component;Ljava/lang/Object;)V",
                (this.clone(), data),
            )
            .await?;

        Ok(true)
    }

    async fn paint_content(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, graphics: ClassInstanceRef<()>) -> JvmResult<()> {
        if graphics.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "").await);
        }

        // The label (with its focus/selection highlight) is drawn by the parent.
        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/LabelComponent",
                "paintContent",
                "(Lorg/kwis/msp/lcdui/Graphics;)V",
                (graphics.clone(),),
            )
            .await?;

        // Frame it so it reads as a button. Red while focused (mask bit 0x2),
        // the muted blue the other components use otherwise.
        let mask: i32 = jvm.get_field(&this, "mask", "I").await?;
        let width: i32 = jvm.get_field(&this, "w", "I").await?;
        let height: i32 = jvm.get_field(&this, "h", "I").await?;

        let border = if mask & 0x2 != 0 { 0x00d2_0000i32 } else { 0x0064_64d2i32 };

        let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (border,)).await?;
        let _: () = jvm
            .invoke_virtual(&graphics, "drawRect", "(IIII)V", (0, 0, width - 1, height - 1))
            .await?;

        Ok(())
    }
}
