use alloc::boxed::Box;

use jvm::{JavaType, JavaValue};

use wie_core_arm::ArmCore;

use super::{KtfJvmWord, array_class_instance::JavaArrayClassInstance, class_instance::JavaClassInstance};

pub trait JavaValueExt {
    fn from_raw(raw: KtfJvmWord, r#type: &JavaType, core: &ArmCore) -> JavaValue;
    fn from_raw64(raw: KtfJvmWord, raw_high: KtfJvmWord, r#type: &JavaType) -> JavaValue;
    fn as_raw(&self) -> KtfJvmWord;
    fn as_raw64(&self) -> (KtfJvmWord, KtfJvmWord);
}

impl JavaValueExt for JavaValue {
    fn from_raw(raw: KtfJvmWord, r#type: &JavaType, core: &ArmCore) -> JavaValue {
        match r#type {
            JavaType::Void => JavaValue::Void,
            JavaType::Boolean => JavaValue::Boolean(raw != 0),
            JavaType::Byte => JavaValue::Byte(raw as i8),
            JavaType::Short => JavaValue::Short(raw as i16),
            JavaType::Int => JavaValue::Int(raw as i32),
            JavaType::Float => JavaValue::Float(f32::from_bits(raw)),
            JavaType::Char => JavaValue::Char(raw as u16),
            JavaType::Class(_) => {
                if raw != 0 {
                    let instance = JavaClassInstance::from_raw(raw, core);
                    // Whether it is really an array, so the array wrapper is used
                    // instead. Reading the class walks the pointer, and a title
                    // can hand a garbage word here where an object belongs (삼국쟁패
                    // does, intermittently, for a String argument). Reading it as
                    // a plain instance rather than unwrapping the read keeps that
                    // a Java-level failure the caller can handle, not a panic that
                    // takes the whole run down.
                    let is_array = instance
                        .class()
                        .ok()
                        .and_then(|class| class.name().ok())
                        .is_some_and(|name| name.starts_with('['));
                    if is_array {
                        let instance = JavaArrayClassInstance::from_raw(raw, core);
                        JavaValue::Object(Some(Box::new(instance)))
                    } else {
                        JavaValue::Object(Some(Box::new(instance)))
                    }
                } else {
                    JavaValue::Object(None)
                }
            }
            JavaType::Array(_) => {
                if raw != 0 {
                    let instance = JavaArrayClassInstance::from_raw(raw, core);

                    JavaValue::Object(Some(Box::new(instance)))
                } else {
                    JavaValue::Object(None)
                }
            }
            _ => panic!(),
        }
    }

    fn from_raw64(raw: KtfJvmWord, raw_high: KtfJvmWord, r#type: &JavaType) -> JavaValue {
        match r#type {
            JavaType::Long => JavaValue::Long((((raw_high as u64) << 32) | raw as u64) as i64),
            JavaType::Double => JavaValue::Double(f64::from_bits(((raw_high as u64) << 32) | raw as u64)),
            _ => panic!(),
        }
    }

    fn as_raw(&self) -> KtfJvmWord {
        match self {
            JavaValue::Void => 0,
            JavaValue::Boolean(x) => *x as _,
            JavaValue::Byte(x) => *x as _,
            JavaValue::Short(x) => *x as _,
            JavaValue::Int(x) => *x as _,
            JavaValue::Float(x) => x.to_bits() as _,
            JavaValue::Char(x) => *x as _,
            JavaValue::Object(x) => {
                if let Some(x) = x {
                    if let Some(x) = x.as_any().downcast_ref::<JavaClassInstance>() {
                        x.ptr_raw as _
                    } else if let Some(x) = x.as_any().downcast_ref::<JavaArrayClassInstance>() {
                        x.class_instance.ptr_raw as _
                    } else {
                        unreachable!()
                    }
                } else {
                    0
                }
            }
            _ => panic!(),
        }
    }

    // (low, high)
    fn as_raw64(&self) -> (KtfJvmWord, KtfJvmWord) {
        match *self {
            JavaValue::Long(x) => ((x as u64 & 0xFFFFFFFF) as _, (x as u64 >> 32) as _),
            JavaValue::Double(x) => ((x.to_bits() & 0xFFFFFFFF) as _, (x.to_bits() >> 32) as _),
            _ => panic!(),
        }
    }
}
