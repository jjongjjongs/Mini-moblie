use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::org::kwis::msp::{lcdui::Display, lwc::Component};

// class org.kwis.msp.lwc.DialogComponent
//
// A modal popup shell. The WIPI UI framework builds confirmation and
// name-entry popups from it; 미니게임천국4 (WEBSYNC1, an LGT title) opens one
// when registering a name, and without the class its native module dies with
// `get_class could not resolve org/kwis/msp/lwc/DialogComponent`
// (NoClassDefFoundError) the moment the popup is created.
//
// It extends ShellComponent, so the title bar, the work component, show/hide,
// the proxy card and the layout are all inherited; what it adds is a modal
// `doModal()` loop that returns which button closed it, the OK/Cancel button
// labels (`setButtonString`), and the dialog-type/result constants the caller
// compares against.
//
// The real class draws its own framed, centred box over the screen and builds
// private OK/Cancel `ButtonComponent`s wired through an inner
// `DialogActionListener`. This runtime instead closes on FIRE (confirm) and
// the right soft key (cancel) in `processEvent`, recognising them before the
// key is forwarded to the focused work component. Every other key - the left
// soft key a name field uses to cycle its input mode, and the digits and CLEAR
// it composes with - is forwarded untouched, so the field keeps working and
// none of them close the dialog. The box is left full-screen like any other
// shell. The field slots are the JVM's own, not the native 44-word layout,
// because nothing reads the dialog's state back through native field access
// (`getActionState`/`actionState` are never imported).
pub struct DialogComponent;

/// `actionState` while the modal loop is still running: no button chosen yet.
const PENDING: i32 = 0;

// The dialog-type and result constants, assigned here (the native values are
// not recorded in the platform table). They only have to be self-consistent:
// the caller sets a type with one and compares `doModal`'s result against the
// others, and both ends are this class.
const TYPE_NONE: i32 = 0;
const TYPE_OK: i32 = 1;
const TYPE_OK_CANCEL: i32 = 2;
const DLG_TIMEOUT: i32 = 3;
const DLG_OK: i32 = 1;
const DLG_CANCEL: i32 = 2;
const OK_BUTTON: i32 = 0;
const CANCEL_BUTTON: i32 = 1;
const TIMEOUT_INFINITE: i32 = -1;

/// The KEY event type for a key going down, as `net.wie.CardCanvas` feeds it
/// in (press = 1, release = 2). The dialog closes only on the press: the key
/// that opened it was pressed on the screen underneath, so the dialog is shown
/// between that press and its release and the release is the first event it
/// sees. Acting on the release would close the dialog the instant it opened.
const KEY_PRESSED: i32 = 1;

/// How long one poll of the modal loop sleeps, in emulated milliseconds. Short
/// enough that a soft-key press is answered promptly, long enough not to spin.
const POLL_MS: u64 = 16;

/// A hard cap on the modal loop, so a dialog that is never answered (a probe
/// with no input, a title that forgets it) cannot wedge the runtime. At
/// [`POLL_MS`] a poll this is many minutes of emulated time - far past any real
/// interaction - and only `TIMEOUT_INFINITE` dialogs reach it.
const MAX_POLLS: u64 = 1_000_000;

impl DialogComponent {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/lwc/DialogComponent",
            parent_class: Some("org/kwis/msp/lwc/ShellComponent"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<clinit>", "()V", Self::cl_init, MethodAccessFlags::STATIC),
                JavaMethodProto::new("<init>", "(I)V", Self::init_type, Default::default()),
                JavaMethodProto::new("<init>", "(Lorg/kwis/msp/lcdui/Display;I)V", Self::init_display_type, Default::default()),
                JavaMethodProto::new(
                    "<init>",
                    "(Lorg/kwis/msp/lwc/Component;Ljava/lang/String;I)V",
                    Self::init_component_title_type,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "<init>",
                    "(Lorg/kwis/msp/lcdui/Display;Lorg/kwis/msp/lwc/Component;Ljava/lang/String;I)V",
                    Self::init_display_component_title_type,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "<init>",
                    "(Lorg/kwis/msp/lwc/Component;Ljava/lang/String;IIIII)V",
                    Self::init_component_title_type_bounds,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "<init>",
                    "(Lorg/kwis/msp/lcdui/Display;Lorg/kwis/msp/lwc/Component;Ljava/lang/String;IIIII)V",
                    Self::init_display_component_title_type_bounds,
                    Default::default(),
                ),
                JavaMethodProto::new("setButtonString", "(ILjava/lang/String;)V", Self::set_button_string, Default::default()),
                JavaMethodProto::new("setType", "(I)V", Self::set_type, Default::default()),
                JavaMethodProto::new("setTimeout", "(I)V", Self::set_timeout, Default::default()),
                JavaMethodProto::new("getTimeout", "()I", Self::get_timeout, Default::default()),
                JavaMethodProto::new("getActionState", "()I", Self::get_action_state, Default::default()),
                JavaMethodProto::new("doModal", "()I", Self::do_modal, Default::default()),
                JavaMethodProto::new("processEvent", "(IIII)Z", Self::process_event, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("TYPE_NONE", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("TYPE_OK", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("TYPE_OK_CANCEL", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("DLG_TIMEOUT", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("DLG_OK", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("DLG_CANCEL", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("OK_BUTTON", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("CANCEL_BUTTON", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("TIMEOUT_INFINITE", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("actionState", "I", Default::default()),
                JavaFieldProto::new("__wieDialogType", "I", Default::default()),
                JavaFieldProto::new("__wieDialogTimeout", "I", Default::default()),
                JavaFieldProto::new("__wieDialogOkLabel", "Ljava/lang/String;", Default::default()),
                JavaFieldProto::new("__wieDialogCancelLabel", "Ljava/lang/String;", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn cl_init(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        let class = "org/kwis/msp/lwc/DialogComponent";
        jvm.put_static_field(class, "TYPE_NONE", "I", TYPE_NONE).await?;
        jvm.put_static_field(class, "TYPE_OK", "I", TYPE_OK).await?;
        jvm.put_static_field(class, "TYPE_OK_CANCEL", "I", TYPE_OK_CANCEL).await?;
        jvm.put_static_field(class, "DLG_TIMEOUT", "I", DLG_TIMEOUT).await?;
        jvm.put_static_field(class, "DLG_OK", "I", DLG_OK).await?;
        jvm.put_static_field(class, "DLG_CANCEL", "I", DLG_CANCEL).await?;
        jvm.put_static_field(class, "OK_BUTTON", "I", OK_BUTTON).await?;
        jvm.put_static_field(class, "CANCEL_BUTTON", "I", CANCEL_BUTTON).await?;
        jvm.put_static_field(class, "TIMEOUT_INFINITE", "I", TIMEOUT_INFINITE).await?;

        Ok(())
    }

    async fn init_type(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, dialog_type: i32) -> JvmResult<()> {
        let display: ClassInstanceRef<Display> = jvm
            .invoke_static("org/kwis/msp/lcdui/Display", "getDefaultDisplay", "()Lorg/kwis/msp/lcdui/Display;", ())
            .await?;

        Self::init_core(
            jvm,
            this,
            display,
            ClassInstanceRef::<Component>::new(None),
            ClassInstanceRef::<String>::new(None),
            dialog_type,
        )
        .await
    }

    async fn init_display_type(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        display: ClassInstanceRef<Display>,
        dialog_type: i32,
    ) -> JvmResult<()> {
        Self::init_core(
            jvm,
            this,
            display,
            ClassInstanceRef::<Component>::new(None),
            ClassInstanceRef::<String>::new(None),
            dialog_type,
        )
        .await
    }

    async fn init_component_title_type(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        component: ClassInstanceRef<Component>,
        title: ClassInstanceRef<String>,
        dialog_type: i32,
    ) -> JvmResult<()> {
        let display: ClassInstanceRef<Display> = jvm
            .invoke_static("org/kwis/msp/lcdui/Display", "getDefaultDisplay", "()Lorg/kwis/msp/lcdui/Display;", ())
            .await?;

        Self::init_core(jvm, this, display, component, title, dialog_type).await
    }

    async fn init_display_component_title_type(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        display: ClassInstanceRef<Display>,
        component: ClassInstanceRef<Component>,
        title: ClassInstanceRef<String>,
        dialog_type: i32,
    ) -> JvmResult<()> {
        Self::init_core(jvm, this, display, component, title, dialog_type).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn init_component_title_type_bounds(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        component: ClassInstanceRef<Component>,
        title: ClassInstanceRef<String>,
        dialog_type: i32,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
    ) -> JvmResult<()> {
        let display: ClassInstanceRef<Display> = jvm
            .invoke_static("org/kwis/msp/lcdui/Display", "getDefaultDisplay", "()Lorg/kwis/msp/lcdui/Display;", ())
            .await?;

        Self::init_core(jvm, this, display, component, title, dialog_type).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn init_display_component_title_type_bounds(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        display: ClassInstanceRef<Display>,
        component: ClassInstanceRef<Component>,
        title: ClassInstanceRef<String>,
        dialog_type: i32,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
    ) -> JvmResult<()> {
        Self::init_core(jvm, this, display, component, title, dialog_type).await
    }

    async fn init_core(
        jvm: &Jvm,
        this: ClassInstanceRef<Self>,
        display: ClassInstanceRef<Display>,
        component: ClassInstanceRef<Component>,
        title: ClassInstanceRef<String>,
        dialog_type: i32,
    ) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lwc.DialogComponent::<init>({this:?}, {component:?}, {title:?}, {dialog_type})");

        // A fixed-size shell: title at top, work component filling the rest.
        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/ShellComponent",
                "<init>",
                "(Lorg/kwis/msp/lcdui/Display;)V",
                (display,),
            )
            .await?;

        let mut this = this;

        jvm.put_field(&mut this, "actionState", "I", PENDING).await?;
        jvm.put_field(&mut this, "__wieDialogType", "I", dialog_type).await?;
        jvm.put_field(&mut this, "__wieDialogTimeout", "I", TIMEOUT_INFINITE).await?;

        if !title.is_null() {
            let _: () = jvm.invoke_virtual(&this, "setTitle", "(Ljava/lang/String;)V", (title,)).await?;
        }

        if !component.is_null() {
            let _: i32 = jvm
                .invoke_virtual(&this, "addComponent", "(Lorg/kwis/msp/lwc/Component;)I", (component,))
                .await?;
        }

        Ok(())
    }

    async fn set_button_string(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        button: i32,
        label: ClassInstanceRef<String>,
    ) -> JvmResult<()> {
        let mut this = this;

        // OK_BUTTON / CANCEL_BUTTON name which label is being set; anything else
        // is ignored, as the native setter does.
        if button == OK_BUTTON {
            jvm.put_field(&mut this, "__wieDialogOkLabel", "Ljava/lang/String;", label).await?;
        } else if button == CANCEL_BUTTON {
            jvm.put_field(&mut this, "__wieDialogCancelLabel", "Ljava/lang/String;", label).await?;
        }

        Ok(())
    }

    async fn set_type(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, dialog_type: i32) -> JvmResult<()> {
        let mut this = this;
        jvm.put_field(&mut this, "__wieDialogType", "I", dialog_type).await?;

        Ok(())
    }

    async fn set_timeout(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, timeout: i32) -> JvmResult<()> {
        let mut this = this;
        jvm.put_field(&mut this, "__wieDialogTimeout", "I", timeout).await?;

        Ok(())
    }

    async fn get_timeout(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        jvm.get_field(&this, "__wieDialogTimeout", "I").await
    }

    async fn get_action_state(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        jvm.get_field(&this, "actionState", "I").await
    }

    /// Shows the dialog and runs a modal loop until a button closes it,
    /// returning which one did.
    ///
    /// The native `doModal` is its own event loop: while it runs, the title's
    /// main loop is parked inside this call and so no longer pumps the event
    /// queue, so the dialog has to pump it itself or no key would ever reach it.
    /// This drives the active Jlet's `EventQueue` exactly as the title's loop
    /// does - `getNextEvent` then `dispatchEvent` - and because `show` pushed
    /// the dialog's card on top, each key `dispatchEvent` delivers reaches this
    /// dialog's `processEvent`, where a soft key sets `actionState`. Repaint
    /// events in the same queue keep the dialog drawn. Without an active Jlet (a
    /// bare unit test) it falls back to a plain sleep-poll.
    async fn do_modal(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("org.kwis.msp.lwc.DialogComponent::doModal({this:?})");

        let mut this_mut = this.clone();
        jvm.put_field(&mut this_mut, "actionState", "I", PENDING).await?;

        let _: () = jvm.invoke_virtual(&this, "show", "()V", ()).await?;

        let timeout: i32 = jvm.get_field(&this, "__wieDialogTimeout", "I").await?;
        let budget = if timeout > 0 {
            ((timeout as u64).div_ceil(POLL_MS)).min(MAX_POLLS)
        } else {
            MAX_POLLS
        };

        let jlet: ClassInstanceRef<()> = jvm
            .invoke_static("org/kwis/msp/lcdui/Jlet", "getActiveJlet", "()Lorg/kwis/msp/lcdui/Jlet;", ())
            .await?;

        let event_queue: ClassInstanceRef<()> = if jlet.is_null() {
            ClassInstanceRef::<()>::new(None)
        } else {
            jvm.invoke_virtual(&jlet, "getEventQueue", "()Lorg/kwis/msp/lcdui/EventQueue;", ())
                .await?
        };

        let event = if event_queue.is_null() {
            None
        } else {
            Some(jvm.instantiate_array("I", 4).await?)
        };

        let mut result = DLG_TIMEOUT;
        for _ in 0..budget {
            let state: i32 = jvm.get_field(&this, "actionState", "I").await?;
            if state != PENDING {
                result = state;
                break;
            }

            if let Some(event) = &event {
                // Pump one event and deliver it, the title's own loop in little.
                let _: () = jvm.invoke_virtual(&event_queue, "getNextEvent", "([I)V", (event.clone(),)).await?;
                let _: () = jvm.invoke_virtual(&event_queue, "dispatchEvent", "([I)V", (event.clone(),)).await?;
            } else {
                context.system().sleep(POLL_MS).await;
            }
        }

        let _: () = jvm.invoke_virtual(&this, "hide", "()V", ()).await?;

        Ok(result)
    }

    /// The dialog result a key closes it with, or `None` if the key is not one
    /// of the dialog's own.
    ///
    /// Only FIRE (the centre/select key, confirming with `DLG_OK`) and the
    /// right soft key (cancelling with `DLG_CANCEL`, or confirming for a dialog
    /// built with no cancel button) are the dialog's. The left soft key is
    /// deliberately absent: a focused name field claims it to cycle its input
    /// mode (Korean/English/symbol), so the dialog must never take it and it is
    /// forwarded to the field like any other editing key.
    async fn dialog_result_for_key(jvm: &Jvm, this: &ClassInstanceRef<Self>, key: i32) -> JvmResult<Option<i32>> {
        // 8 = FIRE, 91 = RIGHT_SOFT_KEY (see Display::getGameAction).
        let action: i32 = jvm.invoke_static("org/kwis/msp/lcdui/Display", "getGameAction", "(I)I", (key,)).await?;

        Ok(match action {
            8 => Some(DLG_OK),
            91 => {
                let dialog_type: i32 = jvm.get_field(this, "__wieDialogType", "I").await?;
                Some(if dialog_type == TYPE_OK_CANCEL { DLG_CANCEL } else { DLG_OK })
            }
            _ => None,
        })
    }

    /// Closes the dialog on FIRE and the right soft key, and forwards every
    /// other key to the focused work component.
    ///
    /// FIRE (confirm) and the right soft key (cancel) are recognised before the
    /// key is forwarded, because no work component wants them and the forward
    /// cannot tell whether one did: `ShellComponent.keyNotify` reports every
    /// key it is handed as handled, so a key the focused field left unhandled
    /// comes back from the superclass looking consumed. The left soft key, the
    /// digits and CLEAR are forwarded untouched, which is what lets the name
    /// field cycle its input mode and compose text; those never close the
    /// dialog.
    ///
    /// Only the press edge closes it ([`KEY_PRESSED`]): the key that opened the
    /// dialog was pressed on the screen underneath, so its release is the first
    /// event this dialog sees and closing on it would make the dialog flash
    /// open and shut.
    async fn process_event(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, event: i32, p1: i32, p2: i32, p3: i32) -> JvmResult<bool> {
        // 3 = KEY; p1 is the press/release type, p2 the key code.
        if event == 3
            && p1 == KEY_PRESSED
            && let Some(state) = Self::dialog_result_for_key(jvm, &this, p2).await?
        {
            let mut this = this;
            jvm.put_field(&mut this, "actionState", "I", state).await?;

            return Ok(true);
        }

        jvm.invoke_special(&this, "org/kwis/msp/lwc/ShellComponent", "processEvent", "(IIII)Z", (event, p1, p2, p3))
            .await
    }
}
