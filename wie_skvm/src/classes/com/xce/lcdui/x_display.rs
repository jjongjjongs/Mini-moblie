use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::canvas::{Clip, Color};
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::javax::microedition::{
    lcdui::{Display, Graphics, Image},
    midlet::MIDlet,
};

// class com.xce.lcdui.XDisplay
pub struct XDisplay;

impl XDisplay {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/xce/lcdui/XDisplay",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<clinit>", "()V", Self::cl_init, MethodAccessFlags::STATIC),
                JavaMethodProto::new("refresh", "(IIII)V", Self::refresh, MethodAccessFlags::STATIC),
                JavaMethodProto::new(
                    "clear",
                    "(Ljavax/microedition/lcdui/Graphics;Ljavax/microedition/lcdui/Image;II)V",
                    Self::clear,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "copyLCD",
                    "(Ljavax/microedition/lcdui/Graphics;Ljavax/microedition/lcdui/Image;IIII)V",
                    Self::copy_lcd,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![
                JavaFieldProto::new("width", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("height", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("height2", "I", FieldAccessFlags::STATIC),
            ],
            access_flags: Default::default(),
        }
    }

    /// The screen's size, which titles read from these fields the way they
    /// would read `Canvas.getWidth` - and cache, in a constructor, for good.
    ///
    /// They were 240x320 whatever the screen was. 사고뭉치트윈즈 centres its
    /// 120x144 play area on `width / 2, height2 / 2`, so run on the 120x160
    /// panel it was made for it still drew around (120, 160), off the bottom
    /// right of the screen. The rest of the screen is its own tiled backdrop,
    /// eight rows of it above and below the play area on that panel.
    ///
    /// `height2` is the whole display too, not the rows a Canvas reports: a
    /// handset kept a soft-key bar there, but titles read `height2` as the
    /// screen - 디지몬RPGII lays its screen out in it and 교실이데아 sizes its
    /// back buffer by it - and the drawing surface here is the whole display
    /// (wfeature answers the same, `publishScreenSize`).
    async fn cl_init(jvm: &Jvm, context: &mut WieJvmContext) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.XDisplay::<clinit>()");

        let (width, height) = {
            let platform = context.system().platform();
            let screen = platform.screen();
            (screen.width() as i32, screen.height() as i32)
        };

        jvm.put_static_field("com/xce/lcdui/XDisplay", "width", "I", width).await?;
        jvm.put_static_field("com/xce/lcdui/XDisplay", "height", "I", height).await?;
        jvm.put_static_field("com/xce/lcdui/XDisplay", "height2", "I", height).await?;

        Ok(())
    }

    async fn refresh(_jvm: &Jvm, context: &mut WieJvmContext, x: i32, y: i32, width: i32, height: i32) -> JvmResult<()> {
        tracing::warn!("stub com.xce.lcdui.XDisplay::refresh({x}, {y}, {width}, {height})");

        let platform = context.system().platform();
        let screen = platform.screen();
        screen.request_redraw().unwrap();

        Ok(())
    }

    /// The vendor's screen clear: black over everything the graphics draws on,
    /// then `image`, when there is one, at (`x`, `y`) as a backdrop.
    ///
    /// It takes no colour - the caller sets one only afterwards, for the text it
    /// writes on the cleared screen - and it clears the whole surface rather
    /// than the clip, as the reference emulator (wfeature, `xDisplayClear`)
    /// reads its one known call site. 코인마스터 names it; it was missing.
    async fn clear(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut graphics: ClassInstanceRef<Graphics>,
        image: ClassInstanceRef<Image>,
        x: i32,
        y: i32,
    ) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.XDisplay::clear({graphics:?}, {image:?}, {x}, {y})");

        if graphics.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "graphics is null").await);
        }

        let target = Graphics::image(jvm, &mut graphics).await?;
        let mut canvas = Image::canvas(jvm, &target).await?;
        let (width, height) = (canvas.image().width(), canvas.image().height());
        let everything = Clip { x: 0, y: 0, width, height };

        canvas.fill_rect(0, 0, width, height, Color { a: 0xff, r: 0, g: 0, b: 0 }, everything);

        if !image.is_null() {
            let backdrop = Image::image(jvm, &image).await?;
            canvas.draw(x, y, backdrop.width(), backdrop.height(), &*backdrop, 0, 0, everything);
        }

        Ok(())
    }

    /// Copies a region of the screen into `image`, at its origin.
    ///
    /// This was a stub, so the image kept whatever it held before - nothing.
    /// 교실이데아 takes the whole screen this way before it draws over it, and
    /// draws the copy back as the background of its field and its dialogue: with
    /// nothing copied, every dialogue sat on black and every sprite that moved
    /// left a trail of itself, since the background drawn over it each frame was
    /// an empty image. The graphics argument is not where the copy comes from;
    /// the screen is, as the reference emulator (wfeature, `xDisplayCopyLCD`)
    /// reads it too.
    #[allow(clippy::too_many_arguments)]
    async fn copy_lcd(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        graphics: ClassInstanceRef<Graphics>,
        image: ClassInstanceRef<Image>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> JvmResult<()> {
        tracing::debug!("com.xce.lcdui.XDisplay::copyLCD({graphics:?}, {image:?}, {x}, {y}, {width}, {height})");

        if image.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "image is null").await);
        }

        Self::copy_screen(jvm, &image, x, y, width, height).await
    }

    /// The screen: the image the current display's canvas paints into and the
    /// host presents.
    async fn screen(jvm: &Jvm) -> JvmResult<ClassInstanceRef<Image>> {
        let midlet: ClassInstanceRef<MIDlet> = jvm
            .get_static_field("javax/microedition/midlet/MIDlet", "currentMIDlet", "Ljavax/microedition/midlet/MIDlet;")
            .await?;
        let display: ClassInstanceRef<Display> = jvm
            .invoke_static(
                "javax/microedition/lcdui/Display",
                "getDisplay",
                "(Ljavax/microedition/midlet/MIDlet;)Ljavax/microedition/lcdui/Display;",
                (midlet,),
            )
            .await?;
        let mut graphics = Display::screen_graphics(jvm, &display).await?;

        Graphics::image(jvm, &mut graphics).await
    }

    /// Copies the screen's `x`, `y`, `width` by `height` into `into` at its
    /// origin. What the region leaves off the screen, or off `into`, is not
    /// copied. The screen is opaque, so what is copied is too.
    pub async fn copy_screen(jvm: &Jvm, into: &ClassInstanceRef<Image>, x: i32, y: i32, width: i32, height: i32) -> JvmResult<()> {
        let screen = Image::image(jvm, &Self::screen(jvm).await?).await?;
        let mut canvas = Image::canvas(jvm, into).await?;

        let (screen_width, screen_height) = (screen.width() as i32, screen.height() as i32);
        let (into_width, into_height) = (canvas.image().width() as i32, canvas.image().height() as i32);

        for row in 0..height.min(into_height) {
            for column in 0..width.min(into_width) {
                let (screen_x, screen_y) = (x + column, y + row);
                if screen_x < 0 || screen_y < 0 || screen_x >= screen_width || screen_y >= screen_height {
                    continue;
                }

                let color = screen.get_pixel(screen_x, screen_y);
                canvas.put_pixel(column, row, Color { a: 0xff, ..color });
            }
        }

        Ok(())
    }
}
