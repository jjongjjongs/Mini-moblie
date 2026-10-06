use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_wipi_java::classes::org::kwis::msp::lwc::Component;

/// KTF's form container, which lays its children out the way an LWC form
/// does.
///
/// 삼국쟁패 패왕전기 keeps one in a field and loads the class when its name
/// entry opens, after the face is picked; with nothing answering it the
/// screen threw `NoClassDefFoundError`. Like `GForm`, it is served by the
/// LWC class it stands for, and a method the title asks for beyond those
/// fails by name.
pub struct GFormComponent;

impl GFormComponent {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GFormComponent",
            parent_class: Some("org/kwis/msp/lwc/FormComponent"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new(
                    "addComponent",
                    "(Lorg/kwis/msp/lwc/Component;IIII)I",
                    Self::add_component_at,
                    Default::default(),
                ),
                JavaMethodProto::new("layout", "()V", Self::layout, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GFormComponent::<init>()");

        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lwc/FormComponent", "<init>", "()V", ()).await?;

        Ok(())
    }

    /// Adds `component` and places it at the bounds given, answering its
    /// index as the one-argument add does. The bounds go through
    /// `Component.configure` with both its position and size bits.
    #[allow(clippy::too_many_arguments)]
    async fn add_component_at(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        component: ClassInstanceRef<Component>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("com.ktf.kfc.GFormComponent::addComponent({component:?}, {x}, {y}, {width}, {height})");

        let index: i32 = jvm
            .invoke_virtual(&this, "addComponent", "(Lorg/kwis/msp/lwc/Component;)I", (component.clone(),))
            .await?;
        let _: () = jvm
            .invoke_virtual(&component, "configure", "(IIIII)V", (x, y, width, height, CONFIGURE_POSITION_AND_SIZE))
            .await?;

        Ok(index)
    }
}

impl GFormComponent {
    /// Keeps each child where `addComponent` put it. The LWC form stacks its
    /// children down its own width, which moved 삼국쟁패's name field from
    /// the box it draws for it to the top of the screen.
    async fn layout(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GFormComponent::layout({this:?})");

        Ok(())
    }
}

/// `Component.configure`'s flags: bit 0 moves it, bit 1 sizes it.
const CONFIGURE_POSITION_AND_SIZE: i32 = 0x3;
