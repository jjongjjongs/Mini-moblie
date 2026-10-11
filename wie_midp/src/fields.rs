//! Reading several fields off one object without resolving its class each time.

use alloc::boxed::Box;

use jvm::{ClassDefinition, ClassInstance, JavaValue, Jvm, Result as JvmResult};

/// Reads one of an instance's own declared fields through a class definition the
/// caller already holds.
///
/// [`Jvm::get_field`] is the general form: it boxes the instance's class
/// definition and the field it finds on every call, and walks the hierarchy for
/// an inherited one. A drawing call reads a dozen fields off the same two
/// objects - the graphics context's translation, clip and mode, the image's
/// size, pitch and pixels - so a title that plots its screen a pixel at a time
/// (귀혼 무사편 blits through `setRGBPixels(x, y, 1, 1, ...)`, thousands of
/// calls a frame) pays for a dozen of those resolutions per pixel.
///
/// A field the class declares itself needs neither the walk nor a second
/// resolution. Anything it does not declare falls back to the general form, so
/// an inherited field still answers.
// The `Box` is the shape `Jvm::get_field` takes, and the fallback hands this
// argument straight to it.
#[allow(clippy::borrowed_box)]
pub(crate) async fn declared_field<T>(
    jvm: &Jvm,
    class: &dyn ClassDefinition,
    instance: &Box<dyn ClassInstance>,
    name: &str,
    descriptor: &str,
) -> JvmResult<T>
where
    T: From<JavaValue>,
{
    match class.field(name, descriptor, false) {
        Some(field) => Ok(instance.get_field(&*field)?.into()),
        None => jvm.get_field(instance, name, descriptor).await,
    }
}
