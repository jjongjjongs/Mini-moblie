use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::canvas::decode_image;
use wie_midp::classes::javax::microedition::lcdui::Image as MidpImage;

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::org::kwis::msp::lcdui::{Card, Display, Graphics, Image};

// class org.kwis.msp.lwc.AnnunciatorComponent
pub struct AnnunciatorComponent;

/// Screen width, bar height, item count, the first state, then the per-item
/// widths, margins and rows, and the atlas the icons are cut from.
type AnnunciatorLayout<'a> = (i32, i32, usize, i32, &'a [i32], &'a [i32], &'a [i32], &'a [u8]);

impl AnnunciatorComponent {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/lwc/AnnunciatorComponent",
            parent_class: Some("org/kwis/msp/lwc/ShellComponent"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<clinit>", "()V", Self::cl_init, MethodAccessFlags::STATIC),
                JavaMethodProto::new("<init>", "(Z)V", Self::init, Default::default()),
                JavaMethodProto::new("<init>", "(Lorg/kwis/msp/lcdui/Display;Z)V", Self::init_with_display, Default::default()),
                JavaMethodProto::new("show", "()V", Self::show, Default::default()),
                JavaMethodProto::new("getHeight", "()I", Self::get_height, Default::default()),
                JavaMethodProto::new("hide", "()V", Self::hide, Default::default()),
                JavaMethodProto::new("layout", "()V", Self::layout, Default::default()),
                JavaMethodProto::new(
                    "addComponent",
                    "(ILorg/kwis/msp/lwc/Component;)V",
                    Self::add_component,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "removeComponent",
                    "(Lorg/kwis/msp/lwc/Component;)V",
                    Self::remove_component,
                    Default::default(),
                ),
                JavaMethodProto::new("paint", "(Lorg/kwis/msp/lcdui/Graphics;)V", Self::paint, Default::default()),
                JavaMethodProto::new("isHorizonEnabled", "()Z", Self::is_horizon_enabled, MethodAccessFlags::STATIC),
            ],
            fields: vec![
                // Native class-static +0x3c.
                JavaFieldProto::new("__wieAnnunciatorSizes", "[I", FieldAccessFlags::STATIC),
                // The atlas the bar's icons are cut from, and which of the six
                // it is. Decoding one costs a 160KB guest image, and the bar is
                // painted with every frame it is on the screen, so it is decoded
                // once and kept: 만귀토벌전 filled its whole heap in eight
                // seconds and died on `Failed to instantiate array` when it was
                // not.
                JavaFieldProto::new("__wieAtlas", "Lorg/kwis/msp/lcdui/Image;", FieldAccessFlags::STATIC),
                JavaFieldProto::new("__wieAtlasIndex", "I", FieldAccessFlags::STATIC),
                // Native AnnunciatorComponent +0x84.
                JavaFieldProto::new("__wieBTrans", "Z", Default::default()),
                // Native AnnunciatorComponent +0x88.
                JavaFieldProto::new("__wieDisplaySizeIndex", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn cl_init(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        // Native AnnunciatorComponent.<clinit> creates int[11]:
        // [14, 20, 24, 24, 0, 20, 24, 48, 48, 48, 48]
        let mut sizes = jvm.instantiate_array("I", 11).await?;
        jvm.store_array(&mut sizes, 0, [14i32, 20, 24, 24, 0, 20, 24, 48, 48, 48, 48]).await?;

        jvm.put_static_field("org/kwis/msp/lwc/AnnunciatorComponent", "__wieAnnunciatorSizes", "[I", sizes)
            .await?;

        // No atlas decoded yet; every real index is zero or above.
        jvm.put_static_field("org/kwis/msp/lwc/AnnunciatorComponent", "__wieAtlasIndex", "I", -1i32)
            .await?;

        Ok(())
    }

    async fn is_horizon_enabled(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<bool> {
        // Native helper @ 0x20f684 always returns true.
        Ok(true)
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<AnnunciatorComponent>, b_trans: bool) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lwc.AnnunciatorComponent::<init>({this:?}, {b_trans})");

        // Native <init>(Z):
        // this(Display.getDefaultDisplay(), bTrans)
        let display: ClassInstanceRef<Display> = jvm
            .invoke_static("org/kwis/msp/lcdui/Display", "getDefaultDisplay", "()Lorg/kwis/msp/lcdui/Display;", ())
            .await?;

        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/AnnunciatorComponent",
                "<init>",
                "(Lorg/kwis/msp/lcdui/Display;Z)V",
                (display, b_trans),
            )
            .await?;

        Ok(())
    }

    async fn init_with_display(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<AnnunciatorComponent>,
        display: ClassInstanceRef<Display>,
        b_trans: bool,
    ) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lwc.AnnunciatorComponent::<init>({this:?}, bTrans={b_trans})");

        // Native Annunciator constructor calls:
        // ShellComponent.<init>(display, false, bTrans)
        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/ShellComponent",
                "<init>",
                "(Lorg/kwis/msp/lcdui/Display;ZZ)V",
                (display.clone(), false, b_trans),
            )
            .await?;

        let width: i32 = jvm.invoke_virtual(&display, "getWidth", "()I", ()).await?;

        // Native +0x88 maps the current Display width to the
        // annunciator size-table index.
        let size_index = match width {
            120 => 0,
            176 => 1,
            240 => 2,
            320 => 3,
            220 => 5,
            400 => 6,
            480 => 7,
            640 => 8,
            800 => 10,
            _ => 0,
        };

        let mut this = this;
        jvm.put_field(&mut this, "__wieBTrans", "Z", b_trans).await?;
        jvm.put_field(&mut this, "__wieDisplaySizeIndex", "I", size_index).await?;

        Ok(())
    }

    async fn show(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<AnnunciatorComponent>) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.lwc.AnnunciatorComponent::show({this:?})");

        // Native order:
        // 1. validate()
        // 2. pushCard(getCard()) when bTrans, otherwise
        //    setDockedCard(getCard(), 0)
        // 3. register a fresh AnnunciatorEventListener.
        //
        // Step 2 is skipped for most titles: the handset's status strip does
        // not appear over a running title on the reference, whichever title it
        // is, so putting the annunciator's card on the screen shows a bar no
        // player sees there and costs the title the rows underneath it. Leaving
        // the card undocked is what makes it invisible - `CardCanvas` paints the
        // docked card and offsets the pushed ones below it, and with none docked
        // every card gets the whole panel from its first row, which is the
        // geometry most titles are written for.
        //
        // A title that lays itself out *below* the strip needs it there, and
        // 만귀토벌전 is one: it clears its client area sixteen rows short of
        // whatever height it is told and puts every screen inside that, so with
        // no strip above it its menus and its battle scene stopped a strip's
        // worth short of the bottom and the rows under them kept the frame
        // before. Docked, its screens land where the handset put them and fill
        // the panel. That is the same fact `wie_wipi_c::api::graphics` reads out
        // of `ANNUNCIATOR_ROWS_PTR` for the titles that reach the strip the
        // other way, so it is the same per-title answer, from `crate::quirks`.
        //
        // Everything a title can observe is still here either way: the component
        // is built, laid out and validated, `getCard` still answers, and the
        // event listener below still runs.
        let _: () = jvm.invoke_virtual(&this, "validate", "()V", ()).await?;

        let display: ClassInstanceRef<Display> = jvm.get_field(&this, "display", "Lorg/kwis/msp/lcdui/Display;").await?;

        if context.system().title_expects_annunciator() {
            let card: ClassInstanceRef<Card> = jvm.invoke_virtual(&this, "getCard", "()Lorg/kwis/msp/lcdui/Card;", ()).await?;

            tracing::debug!("org.kwis.msp.lwc.AnnunciatorComponent::show({this:?}) docks its card");
            let _: () = jvm
                .invoke_virtual(&display, "setDockedCard", "(Lorg/kwis/msp/lcdui/Card;I)V", (card, 0))
                .await?;
        }

        let listener = jvm
            .new_class(
                "org/kwis/msp/lwc/AnnunciatorComponent$AnnunciatorEventListener",
                "(Lorg/kwis/msp/lwc/AnnunciatorComponent;Lorg/kwis/msp/lwc/AnnunciatorComponent$1;)V",
                (this.clone(), None),
            )
            .await?;

        let _: () = jvm
            .invoke_virtual(&display, "addJletEventListener", "(Lorg/kwis/msp/lcdui/JletEventListener;)V", (listener,))
            .await?;

        Ok(())
    }

    /// The rows the strip takes from the title.
    ///
    /// `show` leaves the strip's card undocked, so every card a title pushes
    /// gets the whole panel from its first row. A title that then asks the
    /// strip how tall it is and keeps the rest for itself hands back rows
    /// nothing draws: 블레이드마스터2 asks its card for 240x320, asks this for
    /// 24 and paints `(0, 0, 240, 296)` from then on - and the panel's last 24
    /// rows kept whatever a full-screen picture had left there. 다이어트타이쿤
    /// and 열혈고사전설2 do the same, and the band under each of them was the
    /// title screen it had drawn before.
    ///
    /// So the answer follows where the card is. Undocked - which is how `show`
    /// leaves it - the strip is nowhere on the screen and takes nothing, and
    /// the title draws the whole panel. Docked, by a title that put the card
    /// there itself, it takes its rows as it always did and `CardCanvas` moves
    /// what is pushed below it, so the two answers agree either way.
    ///
    /// Nothing else about the component changes: it is laid out, its card is
    /// the size the table says, and `paint` still draws the bar.
    async fn get_height(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<AnnunciatorComponent>) -> JvmResult<i32> {
        let height: i32 = jvm.get_field(&this, "h", "I").await?;

        let display: ClassInstanceRef<Display> = jvm.get_field(&this, "display", "Lorg/kwis/msp/lcdui/Display;").await?;
        let docked: ClassInstanceRef<Card> = jvm.invoke_virtual(&display, "getDockedCard", "()Lorg/kwis/msp/lcdui/Card;", ()).await?;

        let shown = if docked.is_null() {
            false
        } else {
            let card: ClassInstanceRef<Card> = jvm.invoke_virtual(&this, "getCard", "()Lorg/kwis/msp/lcdui/Card;", ()).await?;
            !card.is_null() && jvm.invoke_virtual(&docked, "equals", "(Ljava/lang/Object;)Z", (card,)).await?
        };

        let height = if shown { height } else { 0 };
        tracing::debug!("org.kwis.msp.lwc.AnnunciatorComponent::getHeight({this:?}) -> {height}");

        Ok(height)
    }

    async fn hide(jvm: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        Err(jvm.exception("java/lang/IllegalStateException", "cannot hide annunciator").await)
    }

    async fn add_component(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        _: ClassInstanceRef<AnnunciatorComponent>,
        _: i32,
        _: ClassInstanceRef<()>,
    ) -> JvmResult<()> {
        Err(jvm.exception("java/lang/IllegalStateException", "cannot add component").await)
    }

    async fn remove_component(jvm: &Jvm, _: &mut WieJvmContext, _: ClassInstanceRef<AnnunciatorComponent>, _: ClassInstanceRef<()>) -> JvmResult<()> {
        Err(jvm.exception("java/lang/IllegalStateException", "cannot remove component").await)
    }

    async fn layout(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<AnnunciatorComponent>) -> JvmResult<()> {
        // Native @ 0x20ef28:
        // configure(0, 0, getWorkComponent().getWidth(), sizes[sizeIndex], 2)
        let work: ClassInstanceRef<()> = jvm
            .invoke_virtual(&this, "getWorkComponent", "()Lorg/kwis/msp/lwc/Component;", ())
            .await?;

        // A shell that has been shown before any work component is added has a
        // null work component. The annunciator bar spans the display, so take
        // the width from the display in that case rather than dereferencing
        // null. The display field is always set (the constructor requires it).
        let width: i32 = if work.is_null() {
            let display: ClassInstanceRef<Display> = jvm.get_field(&this, "display", "Lorg/kwis/msp/lcdui/Display;").await?;
            jvm.invoke_virtual(&display, "getWidth", "()I", ()).await?
        } else {
            jvm.invoke_virtual(&work, "getWidth", "()I", ()).await?
        };

        let size_index: i32 = jvm.get_field(&this, "__wieDisplaySizeIndex", "I").await?;

        let sizes: ClassInstanceRef<Array<i32>> = jvm
            .get_static_field("org/kwis/msp/lwc/AnnunciatorComponent", "__wieAnnunciatorSizes", "[I")
            .await?;

        // `sizes` is an int[]; read the entry for this display size through the
        // element API. The raw byte buffer's read multiplies the offset by the
        // element size itself, so passing a byte offset here double-counted it
        // and ran off the end of the array.
        let heights: alloc::vec::Vec<i32> = jvm.load_array(&sizes, size_index as usize, 1).await?;
        let height = heights.first().copied().unwrap_or(0);

        // A title drawn under a strip of another height says so, and what it
        // was drawn under is what it has to have: 만귀토벌전 keeps 204 rows of
        // its 220-row panel for itself, which leaves sixteen where the table
        // gives a 176-wide panel twenty, and under twenty its last four rows
        // ran off the bottom. See `wie_backend::quirks`.
        let height = context.system().title_annunciator_rows().map_or(height, |rows| rows as i32);

        let _: () = jvm
            .invoke_virtual(&this, "configure", "(IIIII)V", (0i32, 0i32, width, height, 2i32))
            .await?;

        Ok(())
    }

    async fn paint(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<AnnunciatorComponent>,
        graphics: ClassInstanceRef<Graphics>,
    ) -> JvmResult<()> {
        // Native @ 0x20eac8:
        // if (bTrans) graphics.setAlpha(100);
        // paint0(graphics, sizeIndex);
        let b_trans: bool = jvm.get_field(&this, "__wieBTrans", "Z").await?;

        if b_trans {
            let _: () = jvm.invoke_virtual(&graphics, "setAlpha", "(I)V", (100i32,)).await?;
        }

        let size_index: i32 = jvm.get_field(&this, "__wieDisplaySizeIndex", "I").await?;

        Self::paint0(jvm, context, graphics, size_index).await
    }

    /// The atlas for `index`, decoded on the first paint that needs it and kept
    /// from then on.
    ///
    /// One is 160KB of guest heap and the bar is painted with every frame, so
    /// decoding one per paint is a leak the run does not survive - the six are
    /// build-time constants, and only one of them is ever in use.
    async fn atlas(jvm: &Jvm, index: usize, data: &[u8]) -> JvmResult<ClassInstanceRef<Image>> {
        let cached_index: i32 = jvm
            .get_static_field("org/kwis/msp/lwc/AnnunciatorComponent", "__wieAtlasIndex", "I")
            .await?;

        if cached_index == index as i32 {
            let cached: ClassInstanceRef<Image> = jvm
                .get_static_field("org/kwis/msp/lwc/AnnunciatorComponent", "__wieAtlas", "Lorg/kwis/msp/lcdui/Image;")
                .await?;
            if !cached.is_null() {
                return Ok(cached);
            }
        }

        let atlas = Self::load_annunciator_image(jvm, data).await?;

        jvm.put_static_field(
            "org/kwis/msp/lwc/AnnunciatorComponent",
            "__wieAtlas",
            "Lorg/kwis/msp/lcdui/Image;",
            atlas.clone(),
        )
        .await?;
        jvm.put_static_field("org/kwis/msp/lwc/AnnunciatorComponent", "__wieAtlasIndex", "I", index as i32)
            .await?;

        Ok(atlas)
    }

    async fn load_annunciator_image(jvm: &Jvm, data: &[u8]) -> JvmResult<ClassInstanceRef<Image>> {
        let decoded = decode_image(data).map_err(|_| {
            // The embedded resources are build-time constants and validated
            // ECNX blobs. Reaching this path indicates a broken build.
        });

        let decoded = match decoded {
            Ok(decoded) => decoded,
            Err(()) => {
                return Err(jvm
                    .exception("java/lang/IllegalArgumentException", "Failed to decode annunciator image")
                    .await);
            }
        };

        let midp_image = MidpImage::create_image_instance(jvm, decoded.width(), decoded.height(), &decoded.raw(), decoded.bytes_per_pixel()).await?;

        jvm.new_class("org/kwis/msp/lcdui/Image", "(Ljavax/microedition/lcdui/Image;)V", (midp_image,))
            .await
            .map(Into::into)
    }

    async fn paint0(jvm: &Jvm, context: &mut WieJvmContext, graphics: ClassInstanceRef<Graphics>, selector: i32) -> JvmResult<()> {
        // Native commonui_draw_image supports selectors 0,1,2,3,5,6.
        // Constructor indices 7/8/10 have no corresponding native atlas
        // selector and therefore produce no annunciator image.
        let internal_index = match selector {
            0 => 0usize, // 120
            1 => 1usize, // 176
            2 => 2usize, // 240
            3 => 3usize, // 320
            5 => 4usize, // 220
            6 => 5usize, // 400
            _ => return Ok(()),
        };

        const WIDTHS_0: [i32; 13] = [23, 12, 14, 14, 15, 11, 13, 18, 26, 20, 20, 20, 23];
        const WIDTHS_1: [i32; 13] = [26, 20, 20, 20, 23, 21, 21, 25, 33, 20, 22, 24, 25];
        const WIDTHS_2: [i32; 13] = [33, 20, 22, 24, 25, 18, 22, 32, 9, 9, 6, 9, 9];
        const WIDTHS_3: [i32; 13] = [33, 20, 22, 24, 25, 18, 22, 32, 9, 9, 6, 9, 9];

        const MARGINS_0: [i32; 13] = [0; 13];
        const MARGINS_1: [i32; 13] = [0; 13];
        const MARGINS_2: [i32; 13] = [0; 13];
        const MARGINS_3: [i32; 13] = [6, 11, 9, 7, 7, 10, 9, 7, 7, 1, 1, 1, 1];
        const MARGINS_4: [i32; 13] = [5, 5, 5, 5, 5, 5, 5, 5, 9, 18, 18, 18, 18];
        const MARGINS_5: [i32; 13] = [9, 18, 18, 18, 18, 18, 18, 18, 18, 1, 1, 1, 1];

        const ROW_0: [i32; 8] = [8, 7, 6, 5, 4, 3, 2, 1];
        const ROW_1: [i32; 8] = [8, 7, 6, 5, 4, 3, 0, 2];
        const ROW_2: [i32; 13] = [9, 8, 7, 6, 5, 4, 1, 3, 0, 0, 0, 0, 0];
        const ROW_3: [i32; 13] = [9, 8, 7, 6, 5, 4, 1, 3, 0, 0, 0, 0, 0];

        let (screen_width, bar_height, item_count, state0, widths, margins, rows, atlas_data): AnnunciatorLayout = match internal_index {
            0 => (
                120i32,
                14i32,
                8usize,
                6i32,
                &WIDTHS_0,
                &MARGINS_0,
                &ROW_0,
                include_bytes!("resources/annunciator_169x151.ecnx").as_slice(),
            ),
            1 => (
                176i32,
                20i32,
                8usize,
                6i32,
                &WIDTHS_1,
                &MARGINS_1,
                &ROW_1,
                include_bytes!("resources/annunciator_190x211.ecnx").as_slice(),
            ),
            2 => (
                240i32,
                24i32,
                13usize,
                7i32,
                &WIDTHS_2,
                &MARGINS_2,
                &ROW_2,
                include_bytes!("resources/annunciator_273x276.ecnx").as_slice(),
            ),
            3 => (
                320i32,
                24i32,
                13usize,
                7i32,
                &WIDTHS_3,
                &MARGINS_3,
                &ROW_3,
                include_bytes!("resources/annunciator_273x276.ecnx").as_slice(),
            ),
            4 => (
                220i32,
                20i32,
                8usize,
                6i32,
                &WIDTHS_1,
                &MARGINS_4,
                &ROW_1,
                include_bytes!("resources/annunciator_190x211.ecnx").as_slice(),
            ),
            5 => (
                400i32,
                24i32,
                13usize,
                7i32,
                &WIDTHS_3,
                &MARGINS_5,
                &ROW_3,
                include_bytes!("resources/annunciator_273x276.ecnx").as_slice(),
            ),
            _ => unreachable!(),
        };

        // Current WIE system-property backend exposes the same fixed state:
        // ANNUN_CALL/SILENT/ALARM/SMS/SECURITY = 0,
        // ANNUN_CARD is unavailable (native zero/default semantics),
        // BATTERYLEVEL = 100 with MAXBATTLEVEL default 3.
        let mut states = [-1i32; 13];
        states[0] = state0;
        states[1] = -1; // ANNUN_CALL raw 0
        states[2] = -1; // ANNUN_SILENT raw 0 -> raw-1
        states[3] = -1; // ANNUN_ALARM raw 0
        states[4] = -1; // ANNUN_SMS raw 0
        states[5] = -1; // ANNUN_SECURITY raw 0
        states[6] = 0; // ANNUN_CARD raw/default 0
        states[7] = 0; // max(3 - BATTERYLEVEL(100), 0)

        if item_count == 13 {
            // Native fallback is localtime(). WIE's existing LGT localtime
            // implementation currently uses fixed KST (+09:00).
            let epoch_seconds = context.system().platform().now().raw() / 1000;
            let seconds_in_day = (epoch_seconds + 9 * 3600) % 86400;
            let hour = (seconds_in_day / 3600) as i32;
            let minute = ((seconds_in_day % 3600) / 60) as i32;

            let map_digit = |digit: i32| if digit == 0 { 9 } else { digit - 1 };

            states[8] = map_digit(hour / 10);
            states[9] = map_digit(hour % 10);
            states[10] = 10;
            states[11] = map_digit(minute / 10);
            states[12] = map_digit(minute % 10);
        }

        let atlas = Self::atlas(jvm, internal_index, atlas_data).await?;

        // Native background.
        let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (0x00ffffffi32,)).await?;
        let _: () = jvm
            .invoke_virtual(&graphics, "fillRect", "(IIII)V", (0i32, 0i32, screen_width, bar_height - 1))
            .await?;
        let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (0x00a0a0a0i32,)).await?;
        let _: () = jvm
            .invoke_virtual(&graphics, "drawLine", "(IIII)V", (0i32, bar_height - 1, screen_width, bar_height - 1))
            .await?;

        // Native CURRENTCH comparison selects one of two row variants.
        // WIE currently reports CURRENTCH="0"; this follows the normal
        // platform-equal/default row table established from native.
        let mut x = 0i32;

        for item in 0..item_count {
            let state = states[item];
            let width = widths[item];

            if state >= 0 {
                let state_gap = if item == 10 { 4 } else { 1 };
                let source_x = (width + state_gap) * state + 1;
                let source_y = (bar_height + 1) * rows[item] + 1;

                // Draw the atlas source rectangle directly through the
                // MIDP Graphics backing this WIPI Graphics instance.
                let midp_graphics: ClassInstanceRef<()> = jvm.get_field(&graphics, "midpGraphics", "Ljavax/microedition/lcdui/Graphics;").await?;

                let midp_atlas: ClassInstanceRef<MidpImage> = jvm.get_field(&atlas, "midpImage", "Ljavax/microedition/lcdui/Image;").await?;

                let _: () = jvm
                    .invoke_virtual(
                        &midp_graphics,
                        "drawRegion",
                        "(Ljavax/microedition/lcdui/Image;IIIIIIII)V",
                        [
                            midp_atlas.into(),
                            source_x.into(),
                            source_y.into(),
                            width.into(),
                            bar_height.into(),
                            0i32.into(),
                            x.into(),
                            0i32.into(),
                            20i32.into(),
                        ],
                    )
                    .await?;
            }

            x += width + margins[item];
        }

        Ok(())
    }
}
