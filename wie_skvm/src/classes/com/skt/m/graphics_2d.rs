use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::MethodAccessFlags;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_backend::canvas::Color;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};
use wie_midp::classes::javax::microedition::lcdui::{Graphics, Image};

use crate::classes::com::xce::lcdui::XDisplay;

/// SK-VM's `Graphics2D.drawImage` combine modes. 0 is a copy.
const SOURCE_AND: i32 = 1;
const SOURCE_OR: i32 = 2;
const SOURCE_XOR: i32 = 3;

// class com.skt.m.Graphics2D
pub struct Graphics2D;

impl Graphics2D {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/skt/m/Graphics2D",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljavax/microedition/lcdui/Graphics;)V", Self::init, Default::default()),
                JavaMethodProto::new(
                    "getGraphics2D",
                    "(Ljavax/microedition/lcdui/Graphics;)Lcom/skt/m/Graphics2D;",
                    Self::get_graphics2d,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "captureLCD",
                    "(IIII)Ljavax/microedition/lcdui/Image;",
                    Self::capture_lcd,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "drawImage",
                    "(IILjavax/microedition/lcdui/Image;IIIII)V",
                    Self::draw_image,
                    Default::default(),
                ),
                JavaMethodProto::new("invertRect", "(IIII)V", Self::invert_rect, Default::default()),
                JavaMethodProto::new("setPixel", "(III)V", Self::set_pixel, Default::default()),
                JavaMethodProto::new("getPixel", "(II)I", Self::get_pixel, Default::default()),
                JavaMethodProto::new(
                    "createMaskableImage",
                    "(II)Ljavax/microedition/lcdui/Image;",
                    Self::create_maskable_image,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![JavaFieldProto::new("graphics", "Ljavax/microedition/lcdui/Graphics;", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, graphics: ClassInstanceRef<Graphics>) -> JvmResult<()> {
        tracing::debug!("com.skt.m.Graphics2D::<init>({this:?}, {graphics:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "graphics", "Ljavax/microedition/lcdui/Graphics;", graphics)
            .await?;

        Ok(())
    }

    async fn get_graphics2d(jvm: &Jvm, _context: &mut WieJvmContext, graphics: ClassInstanceRef<Graphics>) -> JvmResult<ClassInstanceRef<Self>> {
        tracing::debug!("com.skt.m.Graphics2D::getGraphics2D({graphics:?})");

        let instance = jvm
            .new_class("com/skt/m/Graphics2D", "(Ljavax/microedition/lcdui/Graphics;)V", (graphics,))
            .await?;

        Ok(instance.into())
    }

    /// A new image holding a region of the screen: what a title takes before
    /// it draws an overlay it means to undo. It was a blank image of the right
    /// size, so the undo drew nothing back. `XDisplay.copyLCD` is the same copy
    /// into an image the caller already has.
    async fn capture_lcd(jvm: &Jvm, _context: &mut WieJvmContext, x: i32, y: i32, width: i32, height: i32) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("com.skt.m.Graphics2D::captureLCD({x}, {y}, {width}, {height})");

        if width <= 0 || height <= 0 {
            return Err(jvm
                .exception("java/lang/IllegalArgumentException", "capture width and height must be positive")
                .await);
        }

        let image: ClassInstanceRef<Image> = jvm
            .invoke_static(
                "javax/microedition/lcdui/Image",
                "createImage",
                "(II)Ljavax/microedition/lcdui/Image;",
                (width, height),
            )
            .await?;

        XDisplay::copy_screen(jvm, &image, x, y, width, height).await?;

        Ok(image)
    }

    /// Inverts the colour of every pixel in a rectangle, under the graphics'
    /// translation and clip. 교실이데아 names it; it was missing, which is a
    /// fatal `NoSuchMethodError` the first time it is called.
    async fn invert_rect(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> JvmResult<()> {
        tracing::debug!("com.skt.m.Graphics2D::invertRect({this:?}, {x}, {y}, {width}, {height})");

        let mut graphics: ClassInstanceRef<Graphics> = jvm.get_field(&this, "graphics", "Ljavax/microedition/lcdui/Graphics;").await?;
        let translate_x: i32 = jvm.get_field(&graphics, "translateX", "I").await?;
        let translate_y: i32 = jvm.get_field(&graphics, "translateY", "I").await?;
        let clip = Graphics::clip(jvm, &graphics).await?;

        let image = Graphics::image(jvm, &mut graphics).await?;
        let mut canvas = Image::canvas(jvm, &image).await?;
        let (image_width, image_height) = (canvas.image().width() as i32, canvas.image().height() as i32);

        let left = (x + translate_x).max(clip.x).max(0);
        let top = (y + translate_y).max(clip.y).max(0);
        let right = (x + translate_x + width).min(clip.x + clip.width as i32).min(image_width);
        let bottom = (y + translate_y + height).min(clip.y + clip.height as i32).min(image_height);

        for row in top..bottom {
            for column in left..right {
                let color = canvas.image().get_pixel(column, row);
                canvas.put_pixel(
                    column,
                    row,
                    Color {
                        a: 0xff,
                        r: 0xff - color.r,
                        g: 0xff - color.g,
                        b: 0xff - color.b,
                    },
                );
            }
        }

        Ok(())
    }

    /// Sets one pixel to an RGB colour, under the graphics' translation and
    /// clip. 코인마스터 names it; it was missing.
    async fn set_pixel(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, x: i32, y: i32, rgb: i32) -> JvmResult<()> {
        tracing::debug!("com.skt.m.Graphics2D::setPixel({this:?}, {x}, {y}, {rgb:#x})");

        let mut graphics: ClassInstanceRef<Graphics> = jvm.get_field(&this, "graphics", "Ljavax/microedition/lcdui/Graphics;").await?;
        let translate_x: i32 = jvm.get_field(&graphics, "translateX", "I").await?;
        let translate_y: i32 = jvm.get_field(&graphics, "translateY", "I").await?;
        let clip = Graphics::clip(jvm, &graphics).await?;

        let (x, y) = (x + translate_x, y + translate_y);
        if x < clip.x || x >= clip.x + clip.width as i32 || y < clip.y || y >= clip.y + clip.height as i32 {
            return Ok(());
        }

        let image = Graphics::image(jvm, &mut graphics).await?;
        let mut canvas = Image::canvas(jvm, &image).await?;
        if x < 0 || y < 0 || x >= canvas.image().width() as i32 || y >= canvas.image().height() as i32 {
            return Ok(());
        }

        canvas.put_pixel(
            x,
            y,
            Color {
                a: 0xff,
                r: (rgb >> 16) as u8,
                g: (rgb >> 8) as u8,
                b: rgb as u8,
            },
        );

        Ok(())
    }

    /// One pixel's RGB, under the graphics' translation. A read is bounded by
    /// the surface and not by the clip - a clip is a rule about drawing - and
    /// a pixel off the surface reads as -1, as the reference emulator's does.
    async fn get_pixel(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, x: i32, y: i32) -> JvmResult<i32> {
        tracing::debug!("com.skt.m.Graphics2D::getPixel({this:?}, {x}, {y})");

        let mut graphics: ClassInstanceRef<Graphics> = jvm.get_field(&this, "graphics", "Ljavax/microedition/lcdui/Graphics;").await?;
        let translate_x: i32 = jvm.get_field(&graphics, "translateX", "I").await?;
        let translate_y: i32 = jvm.get_field(&graphics, "translateY", "I").await?;

        let image = Graphics::image(jvm, &mut graphics).await?;
        let pixels = Image::image(jvm, &image).await?;
        let (x, y) = (x + translate_x, y + translate_y);
        if x < 0 || y < 0 || x >= pixels.width() as i32 || y >= pixels.height() as i32 {
            return Ok(-1);
        }

        let color = pixels.get_pixel(x, y);

        Ok(((color.r as i32) << 16) | ((color.g as i32) << 8) | color.b as i32)
    }

    /// Blits a region of `src` with one of SK-VM's combine modes: 0 copies,
    /// and 1, 2 and 3 combine each channel with what is already there by AND,
    /// OR and XOR.
    ///
    /// The mode used to be ignored and every blit was a copy. 디지몬RPGII draws
    /// its cloud shadows as a grey blob on white (`nt/cloud_shadow.lbm`, no
    /// mask) with AND: white leaves the ground as it is and grey darkens it.
    /// Copied, the shadow was a white box round a grey cloud. The modes are the
    /// reference emulator's (wfeature, `graphics2DDrawImage`), which also skips
    /// a transparent source pixel on a copy and combines it on the others.
    #[allow(clippy::too_many_arguments)]
    async fn draw_image(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        tx: i32,
        ty: i32,
        src: ClassInstanceRef<Image>,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        mode: i32,
    ) -> JvmResult<()> {
        tracing::debug!("com.skt.m.Graphics2D::drawImage({this:?}, {tx}, {ty}, {src:?}, {sx}, {sy}, {sw}, {sh}, {mode})");

        if src.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "img is null").await);
        }
        if sw <= 0 || sh <= 0 {
            return Ok(());
        }

        let mut graphics: ClassInstanceRef<Graphics> = jvm.get_field(&this, "graphics", "Ljavax/microedition/lcdui/Graphics;").await?;
        let src_image = Image::image(jvm, &src).await?;

        let translate_x: i32 = jvm.get_field(&graphics, "translateX", "I").await?;
        let translate_y: i32 = jvm.get_field(&graphics, "translateY", "I").await?;
        let (dx, dy) = (tx + translate_x, ty + translate_y);
        let clip = Graphics::clip(jvm, &graphics).await?;

        let image = Graphics::image(jvm, &mut graphics).await?;
        let mut canvas = Image::canvas(jvm, &image).await?;

        let combine: fn(u8, u8) -> u8 = match mode {
            SOURCE_AND => |source, destination| source & destination,
            SOURCE_OR => |source, destination| source | destination,
            SOURCE_XOR => |source, destination| source ^ destination,
            _ => {
                canvas.draw(dx, dy, sw as _, sh as _, &*src_image, sx, sy, clip);
                return Ok(());
            }
        };

        let (width, height) = (canvas.image().width() as i32, canvas.image().height() as i32);
        for row in 0..sh {
            for column in 0..sw {
                let (source_x, source_y) = (sx + column, sy + row);
                if source_x < 0 || source_y < 0 || source_x >= src_image.width() as i32 || source_y >= src_image.height() as i32 {
                    continue;
                }

                let (x, y) = (dx + column, dy + row);
                let inside_clip = x >= clip.x && x < clip.x + clip.width as i32 && y >= clip.y && y < clip.y + clip.height as i32;
                if !inside_clip || x < 0 || y < 0 || x >= width || y >= height {
                    continue;
                }

                let source = src_image.get_pixel(source_x, source_y);
                let destination = canvas.image().get_pixel(x, y);
                canvas.put_pixel(
                    x,
                    y,
                    Color {
                        a: 0xff,
                        r: combine(source.r, destination.r),
                        g: combine(source.g, destination.g),
                        b: combine(source.b, destination.b),
                    },
                );
            }
        }

        Ok(())
    }

    async fn create_maskable_image(jvm: &Jvm, _context: &mut WieJvmContext, width: i32, height: i32) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("com.skt.m.Graphics2D::createMaskableImage({width}, {height})");

        let image: ClassInstanceRef<Image> = jvm
            .invoke_static(
                "javax/microedition/lcdui/Image",
                "createImage",
                "(II)Ljavax/microedition/lcdui/Image;",
                (width, height),
            )
            .await?;

        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

    use test_utils::run_jvm_test;
    use wie_midp::classes::javax::microedition::lcdui::Image;
    use wie_util::Result;

    use crate::get_protos;

    fn protos() -> Box<[Box<[wie_jvm_support::WieJavaClassProto]>]> {
        Box::new([wie_midp::get_protos().into(), get_protos().into()])
    }

    /// A 2x1 image with its two pixels filled with `left` and `right`.
    async fn image(jvm: &Jvm, left: i32, right: i32) -> JvmResult<ClassInstanceRef<Image>> {
        let image: ClassInstanceRef<Image> = jvm
            .invoke_static(
                "javax/microedition/lcdui/Image",
                "createImage",
                "(II)Ljavax/microedition/lcdui/Image;",
                (2, 1),
            )
            .await?;
        let graphics: ClassInstanceRef<()> = jvm
            .invoke_virtual(&image, "getGraphics", "()Ljavax/microedition/lcdui/Graphics;", ())
            .await?;

        for (x, color) in [(0, left), (1, right)] {
            let _: () = jvm.invoke_virtual(&graphics, "setColor", "(I)V", (color,)).await?;
            let _: () = jvm.invoke_virtual(&graphics, "fillRect", "(IIII)V", (x, 0, 1, 1)).await?;
        }

        Ok(image)
    }

    /// Draws `source` over a green destination with `mode` and answers the
    /// destination's two pixels as RGB.
    async fn draw(jvm: &Jvm, source: ClassInstanceRef<Image>, mode: i32) -> JvmResult<[u32; 2]> {
        let destination = image(jvm, 0x40c040, 0x40c040).await?;
        let graphics: ClassInstanceRef<()> = jvm
            .invoke_virtual(&destination, "getGraphics", "()Ljavax/microedition/lcdui/Graphics;", ())
            .await?;
        let graphics_2d: ClassInstanceRef<()> = jvm
            .invoke_static(
                "com/skt/m/Graphics2D",
                "getGraphics2D",
                "(Ljavax/microedition/lcdui/Graphics;)Lcom/skt/m/Graphics2D;",
                (graphics,),
            )
            .await?;

        let _: () = jvm
            .invoke_virtual(
                &graphics_2d,
                "drawImage",
                "(IILjavax/microedition/lcdui/Image;IIIII)V",
                (0, 0, source, 0, 0, 2, 1, mode),
            )
            .await?;

        let pixels = Image::image(jvm, &destination).await?;
        let rgb = |x| {
            let color = pixels.get_pixel(x, 0);
            (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
        };

        Ok([rgb(0), rgb(1)])
    }

    /// AND is how a shadow is drawn: white leaves the ground as it was and
    /// grey darkens it.
    #[test]
    fn and_keeps_the_ground_under_white_and_darkens_it_under_grey() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let shadow = image(&jvm, 0xffffff, 0x808080).await?;

            assert_eq!(draw(&jvm, shadow, 1).await?, [0x40c040, 0x008000]);

            Ok(())
        })
    }

    #[test]
    fn or_and_xor_combine_each_channel() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let source = image(&jvm, 0x800000, 0x40c040).await?;
            assert_eq!(draw(&jvm, source, 2).await?, [0xc0c040, 0x40c040]);

            let source = image(&jvm, 0x800000, 0x40c040).await?;
            assert_eq!(draw(&jvm, source, 3).await?, [0xc0c040, 0x000000]);

            Ok(())
        })
    }

    /// `invertRect` flips each channel under the rectangle and nothing
    /// outside it.
    #[test]
    fn invert_rect_flips_only_the_rectangle() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let image = image(&jvm, 0x40c040, 0x40c040).await?;
            let graphics: ClassInstanceRef<()> = jvm
                .invoke_virtual(&image, "getGraphics", "()Ljavax/microedition/lcdui/Graphics;", ())
                .await?;
            let graphics_2d: ClassInstanceRef<()> = jvm
                .invoke_static(
                    "com/skt/m/Graphics2D",
                    "getGraphics2D",
                    "(Ljavax/microedition/lcdui/Graphics;)Lcom/skt/m/Graphics2D;",
                    (graphics,),
                )
                .await?;

            let _: () = jvm.invoke_virtual(&graphics_2d, "invertRect", "(IIII)V", (1, 0, 1, 1)).await?;

            let pixels = Image::image(&jvm, &image).await?;
            let rgb = |x| {
                let color = pixels.get_pixel(x, 0);
                (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
            };
            assert_eq!([rgb(0), rgb(1)], [0x40c040, 0xbf3fbf]);

            Ok(())
        })
    }

    /// Mode 0 is the copy every blit used to be.
    #[test]
    fn copy_replaces_what_is_there() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            let source = image(&jvm, 0xffffff, 0x808080).await?;

            assert_eq!(draw(&jvm, source, 0).await?, [0xffffff, 0x808080]);

            Ok(())
        })
    }
}
