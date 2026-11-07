//! Reading a field of the class that declares it, rather than of the object.

use alloc::boxed::Box;
use core::fmt::Debug;

use jvm::{ClassInstance, Field, JavaValue, Jvm, Result as JvmResult};

/// Reads the field `class_name` declares, whatever the instance's own class
/// declares by the same name.
///
/// [`Jvm::get_field`] resolves a name against the object's runtime class and
/// walks up from there, so a subclass's field of the same name is the one it
/// finds. Java does not bind a field that way: a field reference names the
/// class that declares it, so a platform class reading its own field gets its
/// own however a subclass names its fields.
///
/// It matters because a title is compiled against the handset's classes and
/// names its own fields freely. 레나크사가's `MainCanvas extends Card`
/// declares `x`, `y`, `w` and `h` of its own - where its camera is and what
/// size it draws - and `Card.repaint` was offsetting its dirty region by that
/// camera. Past the first scene change the camera sits at (-196, -160), so
/// every repaint asked for a region entirely off the screen: the title ran on
/// underneath, loading and drawing, and the screen never changed again.
///
/// A name the class does not declare falls through to the general form, so an
/// inherited field still answers.
///
/// What this buys depends on the JVM under it. A KTF object stores a field at
/// the offset its record carries, so the two `x`es are two words and this
/// reads the right one. The host JVM the other platforms and the tests run on
/// keys an object's storage by the field's name and descriptor alone, so a
/// subclass's `x` and its parent's are one slot there whatever this resolves -
/// which is why the case has no test here and was measured on the title.
// The `Box` is the shape `Jvm::get_field` takes, and the fallback hands this
// argument straight to it.
#[allow(clippy::borrowed_box)]
pub async fn declared_field<T>(jvm: &Jvm, class_name: &str, instance: &Box<dyn ClassInstance>, name: &str, descriptor: &str) -> JvmResult<T>
where
    T: From<JavaValue>,
{
    match declaration(jvm, class_name, name, descriptor).await {
        Some(field) => Ok(instance.get_field(&*field)?.into()),
        None => jvm.get_field(instance, name, descriptor).await,
    }
}

/// Writes the field `class_name` declares. The counterpart of
/// [`declared_field`], and shadowed the same way without it.
pub async fn put_declared_field<T>(
    jvm: &Jvm,
    class_name: &str,
    instance: &mut Box<dyn ClassInstance>,
    name: &str,
    descriptor: &str,
    value: T,
) -> JvmResult<()>
where
    T: Into<JavaValue> + Debug,
{
    match declaration(jvm, class_name, name, descriptor).await {
        Some(field) => instance.put_field(&*field, value.into()),
        None => jvm.put_field(instance, name, descriptor, value).await,
    }
}

/// The field record `class_name` itself declares, if it declares one.
///
/// Only the class's own fields, not its parents': a platform class asks this
/// about a field it declares, and anything else is the general lookup's to
/// answer.
async fn declaration(jvm: &Jvm, class_name: &str, name: &str, descriptor: &str) -> Option<Box<dyn Field>> {
    jvm.resolve_class(class_name).await.ok()?.definition.field(name, descriptor, false)
}
