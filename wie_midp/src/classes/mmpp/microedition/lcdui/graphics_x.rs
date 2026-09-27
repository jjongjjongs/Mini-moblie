use alloc::{boxed::Box, vec};

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstance, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::canvas::Color;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::javax::microedition::lcdui::{Graphics, Image};

// class mmpp.microedition.lcdui.GraphicsX
//
// LG WIPI's extended graphics context. On an LG handset the object a Canvas
// hands to `paint` is a `GraphicsX`, and a title reaches its extended drawing
// through a `(GraphicsX) g` downcast. 호국전기이순신 does exactly that in its
// paint path, so the cast has to succeed against the graphics we deliver.
//
// We model it the way LG's own hierarchy does - as the superclass our
// `javax.microedition.lcdui.Graphics` extends - so every graphics we hand out
// is already a `GraphicsX` and the downcast holds. Most titles draw only
// through the standard `Graphics` methods it inherits; `capture` is the one
// extended entry a title reaches through the cast (지혜의검 saves a strip of the
// screen it is about to draw over), so it lives here where the reference put it.
pub struct GraphicsX;

impl GraphicsX {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/microedition/lcdui/GraphicsX",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("capture", "(IIII)Ljavax/microedition/lcdui/Image;", Self::capture, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.microedition.lcdui.GraphicsX::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        Ok(())
    }

    /// A new image holding a rectangle of what this graphics has drawn: what a
    /// title takes before it paints an overlay it means to undo later. 지혜의검
    /// captures a 99x16 strip in its title paint and blits it back to erase the
    /// blinking "PRESS ANY KEY" between frames. The region is read under the
    /// graphics' translation, the way every other drawing call is.
    async fn capture(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("mmpp.microedition.lcdui.GraphicsX::capture({this:?}, {x}, {y}, {width}, {height})");

        if width <= 0 || height <= 0 {
            return Err(jvm
                .exception("java/lang/IllegalArgumentException", "capture width and height must be positive")
                .await);
        }

        // The object is a `Graphics` (our `Graphics` extends `GraphicsX`); read
        // its backing surface and translation as a `Graphics`.
        let mut graphics: ClassInstanceRef<Graphics> = Into::<Option<Box<dyn ClassInstance>>>::into(this).into();
        let translate_x: i32 = jvm.get_field(&graphics, "translateX", "I").await?;
        let translate_y: i32 = jvm.get_field(&graphics, "translateY", "I").await?;

        let source_image = Graphics::image(jvm, &mut graphics).await?;

        let captured: ClassInstanceRef<Image> = jvm
            .invoke_static(
                "javax/microedition/lcdui/Image",
                "createImage",
                "(II)Ljavax/microedition/lcdui/Image;",
                (width, height),
            )
            .await?;

        let source = Image::image(jvm, &source_image).await?;
        let mut canvas = Image::canvas(jvm, &captured).await?;

        let (source_width, source_height) = (source.width() as i32, source.height() as i32);
        let (captured_width, captured_height) = (canvas.image().width() as i32, canvas.image().height() as i32);

        for row in 0..height.min(captured_height) {
            for column in 0..width.min(captured_width) {
                let (source_x, source_y) = (x + translate_x + column, y + translate_y + row);
                if source_x < 0 || source_y < 0 || source_x >= source_width || source_y >= source_height {
                    continue;
                }

                let color = source.get_pixel(source_x, source_y);
                canvas.put_pixel(column, row, Color { a: 0xff, ..color });
            }
        }

        Ok(captured)
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use jvm::ClassInstanceRef;

    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::{classes::javax::microedition::lcdui::Image, get_protos};

    // Capture is declared on GraphicsX but reached through a Graphics object, so
    // this drives it the way the guest does: an invokevirtual on the Graphics.
    #[test]
    fn test_capture_copies_the_region_it_is_given() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let image: ClassInstanceRef<Image> = jvm
                .invoke_static(
                    "javax/microedition/lcdui/Image",
                    "createImage",
                    "(II)Ljavax/microedition/lcdui/Image;",
                    (20, 20),
                )
                .await?;
            let graphics = jvm
                .new_class(
                    "javax/microedition/lcdui/Graphics",
                    "(Ljavax/microedition/lcdui/Image;)V",
                    (image.clone(),),
                )
                .await?;

            // Paint a red block at (5, 5)-(15, 15); everything else stays the
            // blank image's opaque white.
            let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (0xff0000,)).await?;
            let _: () = jvm.invoke_virtual(&graphics, "fillRect", "(IIII)V", (5, 5, 10, 10)).await?;

            // Capture the 8x8 square at (4, 4): its top-left is white, and one in
            // is the red block.
            let captured: ClassInstanceRef<Image> = jvm
                .invoke_virtual(&graphics, "capture", "(IIII)Ljavax/microedition/lcdui/Image;", (4, 4, 8, 8))
                .await?;

            assert_eq!(jvm.invoke_virtual::<_, i32>(&captured, "getWidth", "()I", ()).await?, 8);
            assert_eq!(jvm.invoke_virtual::<_, i32>(&captured, "getHeight", "()I", ()).await?, 8);

            let backend = Image::image(&jvm, &captured).await?;
            let white = backend.get_pixel(0, 0); // source (4, 4), just outside the block
            let red = backend.get_pixel(1, 1); // source (5, 5), the block's corner
            assert_eq!((white.r, white.g, white.b), (0xff, 0xff, 0xff));
            assert_eq!((red.r, red.g, red.b), (0xff, 0x00, 0x00));

            Ok::<(), jvm::JavaError>(())
        })
    }
}
