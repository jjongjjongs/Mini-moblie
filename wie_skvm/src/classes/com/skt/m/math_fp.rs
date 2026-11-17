use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// SK-VM's fixed-point scale: 1.0 is one billion.
const SCALE: i64 = 1_000_000_000;

// class com.skt.m.MathFP
//
// SK-VM's fixed-point arithmetic, every value a `long` scaled so that 1.0 is
// one billion. Only `parseFPString` used to be here, so a title that did any
// arithmetic stopped on the first call: 코인마스터 dies in `Helper.<clinit>` on
// `parseFP(J)J`, and its battle code goes on to add, sub, multiply, divide,
// sin, cos and sqrt. The set and its meanings are the reference emulator's
// (wfeature, `skvm_register.go`): addition and subtraction work on the scaled
// integers and saturate, everything else goes through a double and saturates on
// the way back.
pub struct MathFP;

impl MathFP {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/skt/m/MathFP",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("parseFPString", "(Ljava/lang/String;)J", Self::parse_fp_string, MethodAccessFlags::STATIC),
                JavaMethodProto::new("parseFP", "(J)J", Self::parse_fp, MethodAccessFlags::STATIC),
                JavaMethodProto::new("toLong", "(J)J", Self::to_long, MethodAccessFlags::STATIC),
                JavaMethodProto::new("abs", "(J)J", Self::abs, MethodAccessFlags::STATIC),
                JavaMethodProto::new("round", "(J)J", Self::round, MethodAccessFlags::STATIC),
                JavaMethodProto::new("sin", "(J)J", Self::sin, MethodAccessFlags::STATIC),
                JavaMethodProto::new("cos", "(J)J", Self::cos, MethodAccessFlags::STATIC),
                JavaMethodProto::new("tan", "(J)J", Self::tan, MethodAccessFlags::STATIC),
                JavaMethodProto::new("asin", "(J)J", Self::asin, MethodAccessFlags::STATIC),
                JavaMethodProto::new("acos", "(J)J", Self::acos, MethodAccessFlags::STATIC),
                JavaMethodProto::new("atan", "(J)J", Self::atan, MethodAccessFlags::STATIC),
                JavaMethodProto::new("exp", "(J)J", Self::exp, MethodAccessFlags::STATIC),
                JavaMethodProto::new("log", "(J)J", Self::log, MethodAccessFlags::STATIC),
                JavaMethodProto::new("sqrt", "(J)J", Self::sqrt, MethodAccessFlags::STATIC),
                JavaMethodProto::new("add", "(JJ)J", Self::add, MethodAccessFlags::STATIC),
                JavaMethodProto::new("sub", "(JJ)J", Self::sub, MethodAccessFlags::STATIC),
                JavaMethodProto::new("multiply", "(JJ)J", Self::multiply, MethodAccessFlags::STATIC),
                JavaMethodProto::new("divide", "(JJ)J", Self::divide, MethodAccessFlags::STATIC),
                JavaMethodProto::new("max", "(JJ)J", Self::max, MethodAccessFlags::STATIC),
                JavaMethodProto::new("min", "(JJ)J", Self::min, MethodAccessFlags::STATIC),
                JavaMethodProto::new("pow", "(JJ)J", Self::pow, MethodAccessFlags::STATIC),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn parse_fp_string(jvm: &Jvm, _context: &mut WieJvmContext, s: ClassInstanceRef<String>) -> JvmResult<i64> {
        tracing::debug!("com.skt.m.MathFP::parseFPString({s:?})");

        let text = JavaLangString::to_rust_string(jvm, &s).await?;
        let Ok(value) = text.trim().parse::<f64>() else {
            return Err(jvm.exception("java/lang/NumberFormatException", &text).await);
        };

        Ok(from_float(value))
    }

    /// A whole number, scaled into the fixed representation.
    async fn parse_fp(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(saturate(value as f64 * SCALE as f64))
    }

    /// The whole part, the fraction dropped toward zero.
    async fn to_long(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(value / SCALE)
    }

    async fn abs(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(value.checked_abs().unwrap_or(i64::MAX))
    }

    async fn round(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::round(to_float(value))))
    }

    async fn sin(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::sin(to_float(value))))
    }

    async fn cos(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::cos(to_float(value))))
    }

    async fn tan(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::tan(to_float(value))))
    }

    async fn asin(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::asin(to_float(value))))
    }

    async fn acos(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::acos(to_float(value))))
    }

    async fn atan(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::atan(to_float(value))))
    }

    async fn exp(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::exp(to_float(value))))
    }

    async fn log(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::log(to_float(value))))
    }

    async fn sqrt(_: &Jvm, _: &mut WieJvmContext, value: i64) -> JvmResult<i64> {
        Ok(from_float(libm::sqrt(to_float(value))))
    }

    /// Adding two fixed values needs no rescaling, and going through a double
    /// would lose the low digits of a large one.
    async fn add(_: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        Ok(a.saturating_add(b))
    }

    async fn sub(_: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        Ok(a.saturating_sub(b))
    }

    async fn multiply(_: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        Ok(from_float(to_float(a) * to_float(b)))
    }

    async fn divide(jvm: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        if b == 0 {
            return Err(jvm.exception("java/lang/ArithmeticException", "division by zero").await);
        }

        Ok(from_float(to_float(a) / to_float(b)))
    }

    async fn max(_: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        Ok(a.max(b))
    }

    async fn min(_: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        Ok(a.min(b))
    }

    async fn pow(_: &Jvm, _: &mut WieJvmContext, a: i64, b: i64) -> JvmResult<i64> {
        Ok(from_float(libm::pow(to_float(a), to_float(b))))
    }
}

fn to_float(value: i64) -> f64 {
    value as f64 / SCALE as f64
}

fn from_float(value: f64) -> i64 {
    saturate(value * SCALE as f64)
}

/// A computed double back to a fixed value, clamped rather than wrapped: a title
/// that overflows an intermediate should see a huge number, not a sign flip.
fn saturate(value: f64) -> i64 {
    if value.is_nan() {
        0
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else {
        value as i64
    }
}

#[cfg(test)]
mod tests {
    use super::{SCALE, from_float, saturate, to_float};

    #[test]
    fn one_is_a_billion_and_the_conversions_round_trip() {
        assert_eq!(from_float(1.5), 1_500_000_000);
        assert_eq!(to_float(2 * SCALE), 2.0);
        assert_eq!(saturate(f64::MAX), i64::MAX);
        assert_eq!(saturate(f64::NAN), 0);
    }
}
