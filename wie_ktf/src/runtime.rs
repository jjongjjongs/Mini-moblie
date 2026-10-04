mod init;
mod java;
mod svc_ids;
mod wipi_c;

const SVC_CATEGORY_INIT: u32 = 1;
const SVC_CATEGORY_JAVA_INTERFACE: u32 = 2;
const SVC_CATEGORY_WIPIC: u32 = 3;
const SVC_CATEGORY_JAVA: u32 = 4;
const SVC_CATEGORY_MODULE: u32 = 5;
const SVC_CATEGORY_MODULE_CLASS: u32 = 6;
const SVC_CATEGORY_MODULE_JUMP: u32 = 7;

pub use self::java::jvm_support::KtfJvmSupport;
